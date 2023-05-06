use std::iter::repeat;

use eyre::{Context, Result};
use image::GrayImage;
use tracing::debug;

use crate::{util, Nibble};

const PATTERN_COUNT: usize = 98;

const CONTROL_DATA_SIZE: usize = 23;
const SERIALIZED_DATA_PATTERN_LIST_LENGTH: usize = 686;

pub struct Pattern {
    number: u16,
    rows: Vec<Vec<bool>>,
    height: u16,
    width: u16,
    memo: Vec<u8>,
}

#[derive(Default, Debug)]
struct ControlData {
    next_pattern_ptr1: u16,
    unknown1: u16,
    next_pattern_ptr2: u16,
    last_pattern_end_ptr: u16,
    unknown2: u16,
    last_pattern_start_ptr: u16,
    unknown3: u32,
    header_end_ptr: u16,
    unknown_ptr: u16,
    unknown4_1: u16,
    unknown4_2: u8,
}

pub struct MachineState {
    patterns: Vec<Pattern>,
    data0: Vec<u8>,
    control_data: ControlData,
    data1: Vec<u8>,
    loaded_pattern: u16,
    data2: Vec<u8>,
}

impl MachineState {
    pub fn from_memory_dump(data: &[u8]) -> Self {
        let mut patterns = Vec::new();

        for i in 0..PATTERN_COUNT {
            if let Some(pattern) = Pattern::from_memory_dump(data, i) {
                patterns.push(pattern);
            }
        }

        let data0 = data[0x7ee0..0x7f00].to_vec();
        let control_data = ControlData::from_memory_dump(&data[0x7f00..0x7f17]);

        debug!(?control_data, "Control data parsed");

        let data1 = data[0x7f17..0x7fea].to_vec();
        let loaded_pattern = util::from_bcd(&util::to_nibbles(&data[0x7fea..0x7fec])[1..]);
        let data2 = data[0x7fec..0x8000].to_vec();

        MachineState {
            patterns,
            data0,
            control_data,
            data1,
            loaded_pattern,
            data2,
        }
    }

    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }

    pub fn add_pattern(&mut self, pattern: Pattern) {
        self.patterns.retain(|p| p.number != pattern.number);
        self.patterns.push(pattern);
        self.patterns.sort_unstable_by_key(|p| p.number);
    }

    pub fn serialize(&mut self) -> Vec<u8> {
        let pattern_layout = {
            let mut offset = 0x120;
            let mut layout = Vec::with_capacity(self.patterns.len());

            for pattern in &self.patterns {
                let data = pattern.serialize_data();
                let data_len = data.len() as u16;
                layout.push((offset, pattern, data));
                offset += data_len;
            }

            layout
        };

        self.control_data.update(&pattern_layout);

        let pattern_layout_data = serialize_pattern_layout(&pattern_layout);
        let pattern_mem_pad = serialize_pattern_memory_padding(&pattern_layout);
        let pattern_mem = serialize_pattern_memory(&pattern_layout);
        let control_data = self.control_data.serialize();
        let loaded_pattern = serialize_loaded_pattern(self.loaded_pattern);

        let mut data = vec![];

        data.extend(pattern_layout_data);
        data.extend(pattern_mem_pad);
        data.extend(pattern_mem);
        data.extend(&self.data0);
        data.extend(control_data);
        data.extend(&self.data1);
        data.extend(loaded_pattern);
        data.extend(&self.data2);

        assert_eq!(data.len(), 32768);

        data
    }
}

impl Pattern {
    fn from_memory_dump(data: &[u8], index: usize) -> Option<Self> {
        let header = &data[index * 7..(index + 1) * 7];

        let end_offset = u16::from_be_bytes([header[0], header[1]]);
        if end_offset == 0 {
            return None;
        }

        let data_nibbles = util::to_nibbles(&header[2..]);
        let height = util::from_bcd(&data_nibbles[0..3]);
        let width = util::from_bcd(&data_nibbles[3..6]);
        let ptn_num = util::from_bcd(&data_nibbles[7..10]);

        debug!(
            ?index,
            ?width,
            ?height,
            ?ptn_num,
            ?end_offset,
            "Found pattern"
        );

        let memo_size = memo_size(height);
        let memo_end_pos = 0x7fff - end_offset as usize;
        let memo_start_pos = memo_end_pos - memo_size;

        let memo = &data[memo_start_pos + 1..memo_end_pos + 1];

        debug!("Memo data: {memo:x?}");

        let pattern_size =
            ((f32::from(width) / 4.0).ceil() * f32::from(height) / 2.0).ceil() as usize;
        let pattern_end_pos = memo_start_pos;
        let pattern_start_pos = pattern_end_pos - pattern_size;

        let pattern = &data[pattern_start_pos + 1..pattern_end_pos + 1];

        debug!("Pattern data: {pattern:x?}");

        let parsed_pattern = parse_pattern_rows(width, height, pattern);

        for row in &parsed_pattern {
            for col in row.iter().copied() {
                if col {
                    print!("X");
                } else {
                    print!("_");
                }
            }

            println!();
        }

        Some(Pattern {
            number: ptn_num,
            rows: parsed_pattern,
            height,
            width,
            memo: memo.to_vec(),
        })
    }

    pub fn from_image(pattern_number: u16, image: &GrayImage) -> Result<Self> {
        let width = u16::try_from(image.width()).context("Image too wide")?;
        let height = u16::try_from(image.height()).context("Image too wide")?;

        let memo_size = memo_size(height);
        let memo = vec![0; memo_size];

        let mut rows = vec![vec![false; width as usize]; height as usize];

        for y in 0..height {
            for x in 0..width {
                let color = image.get_pixel(x.into(), y.into())[0] < 128;
                rows[y as usize][x as usize] = color;
            }
        }

        Ok(Pattern {
            number: pattern_number,
            rows,
            height,
            width,
            memo,
        })
    }

    pub fn pattern_number(&self) -> u16 {
        self.number
    }

    pub fn to_image(&self) -> GrayImage {
        let mut image = GrayImage::new(u32::from(self.width), u32::from(self.height));

        for (y, row) in self.rows.iter().enumerate() {
            for (x, col) in row.iter().copied().enumerate() {
                let color = if col { 0 } else { 255 };
                *image.get_pixel_mut(x as u32, y as u32) = [color].into();
            }
        }

        image
    }

    fn serialize_header(&self, offset: u16) -> Vec<u8> {
        let mut data = vec![0, 0];
        data[0..2].copy_from_slice(&offset.to_be_bytes());

        let mut header_nibbles = Vec::with_capacity(10);
        header_nibbles.extend(util::to_bcd(self.height, 3));
        header_nibbles.extend(util::to_bcd(self.width, 3));
        header_nibbles.extend(util::to_bcd(self.number, 4));

        data.extend(util::from_nibbles(&header_nibbles));

        data
    }

    fn serialize_data(&self) -> Vec<u8> {
        let (_, row_pad_bits, initial_padding) = pattern_data_sizes(self.width, self.height);

        let mut bits = vec![false; initial_padding * 4];

        for row in &self.rows {
            bits.extend(repeat(false).take(row_pad_bits));
            bits.extend(row.iter().copied().rev());
        }

        let mut serialized = util::bits_to_bytes(&bits);
        serialized.extend(&self.memo);
        serialized
    }
}

impl ControlData {
    fn from_memory_dump(data: &[u8]) -> ControlData {
        assert_eq!(data.len(), CONTROL_DATA_SIZE);

        ControlData {
            next_pattern_ptr1: u16::from_be_bytes([data[0], data[1]]),
            unknown1: u16::from_be_bytes([data[2], data[3]]),
            next_pattern_ptr2: u16::from_be_bytes([data[4], data[5]]),
            last_pattern_end_ptr: u16::from_be_bytes([data[6], data[7]]),
            unknown2: u16::from_be_bytes([data[8], data[9]]),
            last_pattern_start_ptr: u16::from_be_bytes([data[10], data[11]]),
            unknown3: u32::from_be_bytes([data[12], data[13], data[14], data[15]]),
            header_end_ptr: u16::from_be_bytes([data[16], data[17]]),
            unknown_ptr: u16::from_be_bytes([data[18], data[19]]),
            unknown4_1: u16::from_be_bytes([data[20], data[21]]),
            unknown4_2: data[22],
        }
    }

    fn update(&mut self, pattern_layout: &[(u16, &Pattern, Vec<u8>)]) {
        let last_pattern_start;
        let last_pattern_end;
        let next_pattern_ptr;

        if let Some((end, _, data)) = pattern_layout.last() {
            last_pattern_end = *end;
            last_pattern_start = *end + data.len() as u16;
            next_pattern_ptr = last_pattern_start + 1;
        } else {
            next_pattern_ptr = 0x120;
            last_pattern_start = 0;
            last_pattern_end = 0;
        }

        self.next_pattern_ptr1 = next_pattern_ptr;
        self.next_pattern_ptr2 = if pattern_layout.is_empty() {
            0
        } else {
            next_pattern_ptr
        };
        self.last_pattern_end_ptr = last_pattern_end;
        self.last_pattern_start_ptr = last_pattern_start;
        self.header_end_ptr = (0x8000 - (7 * pattern_layout.len()) - 7) as u16;
    }

    fn serialize(&self) -> [u8; CONTROL_DATA_SIZE] {
        let mut data = [0; CONTROL_DATA_SIZE];

        data[0..2].copy_from_slice(&self.next_pattern_ptr1.to_be_bytes());
        data[2..4].copy_from_slice(&self.unknown1.to_be_bytes());
        data[4..6].copy_from_slice(&self.next_pattern_ptr2.to_be_bytes());
        data[6..8].copy_from_slice(&self.last_pattern_end_ptr.to_be_bytes());
        data[8..10].copy_from_slice(&self.unknown2.to_be_bytes());
        data[10..12].copy_from_slice(&self.last_pattern_start_ptr.to_be_bytes());
        data[12..16].copy_from_slice(&self.unknown3.to_be_bytes());
        data[16..18].copy_from_slice(&self.header_end_ptr.to_be_bytes());
        data[18..20].copy_from_slice(&self.unknown_ptr.to_be_bytes());
        data[20..22].copy_from_slice(&self.unknown4_1.to_be_bytes());
        data[22] = self.unknown4_2;

        data
    }
}

fn memo_size(height: u16) -> usize {
    (if height % 2 == 0 {
        height / 2
    } else {
        height / 2 + 1
    }) as usize
}

fn pattern_data_sizes(width: u16, height: u16) -> (usize, usize, usize) {
    let row_nibbles = (f32::from(width) / 4.0).ceil() as usize;
    let row_pad_bits = util::padding(usize::from(width), 4);

    let initial_padding = util::padding(row_nibbles * usize::from(height), 2);

    (row_nibbles, row_pad_bits, initial_padding)
}

fn parse_pattern_rows(width: u16, height: u16, data: &[u8]) -> Vec<Vec<bool>> {
    let (row_nibbles, row_pad_bits, initial_padding) = pattern_data_sizes(width, height);

    let nibble_data = util::to_nibbles(data);

    (0..usize::from(height))
        .map(|row| {
            let start_index = initial_padding + row_nibbles * row;
            let end_index = start_index + row_nibbles;

            let bits = util::nibble_bits(&nibble_data[start_index..end_index]);

            bits[row_pad_bits..].iter().copied().rev().collect()
        })
        .collect()
}

fn serialize_pattern_layout(layout: &[(u16, &Pattern, Vec<u8>)]) -> Vec<u8> {
    let mut data = vec![];

    for (offset, pattern, _) in layout {
        data.extend(pattern.serialize_header(*offset));
    }

    let max_number = layout.iter().map(|(_, p, _)| p.number).max().unwrap_or(900);

    data.extend([0, 0, 0, 0, 0]);
    data.extend(util::from_nibbles(&util::to_bcd(max_number + 1, 4)));

    let pad_patterns = 97 - layout.len();
    data.extend(repeat(0).take(pad_patterns * 7));

    assert_eq!(data.len(), SERIALIZED_DATA_PATTERN_LIST_LENGTH);

    data
}

fn serialize_pattern_memory_padding(layout: &[(u16, &Pattern, Vec<u8>)]) -> Vec<u8> {
    let last_pattern_end;

    if let Some((end, _, data)) = layout.last() {
        last_pattern_end = *end as usize + data.len();
    } else {
        last_pattern_end = 0x120;
    }

    let pattern_pad = 0x8000 - last_pattern_end - SERIALIZED_DATA_PATTERN_LIST_LENGTH;

    vec![0; pattern_pad]
}

fn serialize_pattern_memory(layout: &[(u16, &Pattern, Vec<u8>)]) -> Vec<u8> {
    let mut data = Vec::with_capacity(layout.len() * SERIALIZED_DATA_PATTERN_LIST_LENGTH);

    for (_, _, pattern_data) in layout.iter().rev() {
        data.extend(pattern_data);
    }

    data
}

fn serialize_loaded_pattern(pattern: u16) -> Vec<u8> {
    let mut nibbles = vec![Nibble::new(1)];
    nibbles.extend(util::to_bcd(pattern, 3));
    util::from_nibbles(&nibbles)
}
