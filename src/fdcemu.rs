use std::{
    convert::Infallible,
    fs::File,
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

use eyre::{bail, ensure, Result};
use serial::{PortSettings, SerialPort};
use tracing::{debug, trace};

const SECTOR_ID_LEN: usize = 12;
const SECTOR_DATA_LEN: usize = 1024;

const SECTOR_COUNT: usize = 80;

#[derive(Clone)]
struct Sector {
    id: [u8; SECTOR_ID_LEN],
    data: [u8; SECTOR_DATA_LEN],
}

pub struct Disk {
    sectors: Box<[Sector; SECTOR_COUNT]>,
}

enum FdcMode {
    Op,
    Fdc,
}

pub struct FdcServer<P: SerialPort> {
    port: P,
    mode: FdcMode,
    disk: Disk,
    disk_path: PathBuf,
}

impl Sector {
    const EMPTY: Sector = Sector {
        id: [0; SECTOR_ID_LEN],
        data: [0; SECTOR_DATA_LEN],
    };
}

impl Disk {
    pub fn new() -> Self {
        Disk {
            sectors: Box::new([Sector::EMPTY; SECTOR_COUNT]),
        }
    }

    pub fn flatten_data(&self) -> Vec<u8> {
        let mut data = Vec::with_capacity(SECTOR_COUNT * SECTOR_DATA_LEN);

        for sector in self.sectors.iter() {
            data.extend(sector.data);
        }

        data
    }

    pub fn set_flattened_data(&mut self, mut data: Vec<u8>) -> Result<()> {
        data.resize(SECTOR_COUNT * SECTOR_DATA_LEN, 0);

        for (i, sector) in self.sectors.iter_mut().enumerate() {
            let start_index = i * SECTOR_DATA_LEN;
            let end_index = (i + 1) * SECTOR_DATA_LEN;
            sector.data.copy_from_slice(&data[start_index..end_index]);
        }

        Ok(())
    }

    pub fn load(&mut self, path: &Path) -> Result<()> {
        let mut f = BufReader::new(File::open(path)?);

        for sector in self.sectors.iter_mut() {
            f.read_exact(&mut sector.id)?;
            f.read_exact(&mut sector.data)?;
        }

        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut f = BufWriter::new(File::create(path)?);

        for sector in self.sectors.iter() {
            f.write_all(&sector.id)?;
            f.write_all(&sector.data)?;
        }

        Ok(())
    }
}

impl<P: SerialPort> FdcServer<P> {
    pub fn new(disk_path: &Path, mut port: P) -> Result<Self> {
        port.configure(&PortSettings {
            baud_rate: serial::BaudRate::Baud9600,
            char_size: serial::CharSize::Bits8,
            parity: serial::Parity::ParityNone,
            stop_bits: serial::StopBits::Stop1,
            flow_control: serial::FlowControl::FlowNone,
        })?;
        port.set_rts(true)?;
        port.set_timeout(Duration::from_secs(3600))?;

        let mut disk = Disk::new();

        if disk_path.exists() {
            disk.load(disk_path)?;
        }

        Ok(FdcServer {
            port,
            mode: FdcMode::Op,
            disk,
            disk_path: disk_path.to_owned(),
        })
    }

    pub fn run(&mut self) -> Result<Infallible> {
        loop {
            self.step()?;

            self.disk.save(&self.disk_path)?;
        }
    }

    fn step(&mut self) -> Result<()> {
        match self.mode {
            FdcMode::Op => self.step_op(),
            FdcMode::Fdc => self.step_fdc(),
        }
    }

    fn step_op(&mut self) -> Result<()> {
        let zz = read_nonzero(&mut self.port, 2)?;
        if zz != [b'Z', b'Z'] {
            bail!("Expected ZZ ({:x?}), got {zz:x?}", [b'Z', b'Z']);
        }

        self.handle_op_mode_request()
    }

    #[tracing::instrument(skip(self))]
    fn handle_op_mode_request(&mut self) -> Result<()> {
        let cmd = read_single(&mut self.port)?;
        let datalen = read_single(&mut self.port)?;
        let mut data = vec![0; datalen as usize];
        self.port.read_exact(&mut data)?;
        let expected_checksum = read_single(&mut self.port)?;

        println!("OP: cmd={cmd:x}, datalen={datalen}, expected_checksum={expected_checksum:x}, data={data:x?}");

        match cmd {
            0x8 => {
                self.mode = FdcMode::Fdc;
                Ok(())
            }
            _ => {
                bail!("Unknown command in OP mode: {cmd:x}");
            }
        }
    }

    fn step_fdc(&mut self) -> Result<()> {
        let cmd = read_single(&mut self.port)?;

        match cmd {
            b'\r' => Ok(()),
            b'Z' => self.fdc_op_mode_request(),
            b'A' => self.fdc_read_id_section(),
            b'S' => self.fdc_search_id_section(),
            b'B' | b'C' => self.fdc_write_id_section(),
            b'W' | b'X' => self.fdc_write_sector(),
            b'R' => self.fdc_read_sector(),
            _ => bail!("Unknown command in FDC mode: {cmd:x}"),
        }
    }

    #[tracing::instrument(skip(self))]
    fn fdc_op_mode_request(&mut self) -> Result<()> {
        let cmd = read_single(&mut self.port)?;
        if cmd == b'Z' {
            self.mode = FdcMode::Op;
            self.handle_op_mode_request()
        } else {
            bail!("Got 'Z' in FDC mode but not followed by another 'Z', got: {cmd:x?}")
        }
    }

    #[tracing::instrument(skip(self))]
    fn fdc_read_id_section(&mut self) -> Result<()> {
        let args = self.read_fdc_args()?;
        let (psn, _) = parse_psn_lsn(&args)?;

        let response = format!("00{psn:02X}0000");
        self.port.write_all(response.as_bytes())?;

        let wait_value = read_single(&mut self.port)?;
        ensure!(wait_value == b'\r', "Expected \\r, got {wait_value:x}");

        let sector = &self.disk.sectors[psn as usize];
        self.port.write_all(&sector.id)?;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn fdc_search_id_section(&mut self) -> Result<()> {
        let args = self.read_fdc_args()?;
        ensure!(
            args.is_empty(),
            "There should be no args provided to search_id"
        );

        self.port.write_all(b"00000000")?;

        let mut sector_id = [0; SECTOR_ID_LEN];
        self.port.read_exact(&mut sector_id)?;

        debug!("Trying to find sector with ID {sector_id:02x?}");

        if let Some(sector_index) = self
            .disk
            .sectors
            .iter()
            .position(|sector| sector.id == sector_id)
        {
            debug!("  Found at index {sector_index}");
            let buffer = format!("00{sector_index:02X}0000");
            self.port.write_all(buffer.as_bytes())?;
        } else {
            debug!("  Not found");
            self.port.write_all(b"40000000")?;
        }

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn fdc_write_id_section(&mut self) -> Result<()> {
        let args = self.read_fdc_args()?;
        let (psn, _) = parse_psn_lsn(&args)?;

        self.port.write_all(format!("00{psn:02X}0000").as_bytes())?;

        let mut sector_id = [0; SECTOR_ID_LEN];
        self.port.read_exact(&mut sector_id)?;

        debug!("Setting sector ID for index {psn} to {sector_id:02x?}");

        let mut sector = &mut self.disk.sectors[psn as usize];
        sector.id = sector_id;

        self.port.write_all(format!("00{psn:02X}0000").as_bytes())?;

        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn fdc_write_sector(&mut self) -> Result<()> {
        let args = self.read_fdc_args()?;
        let (psn, _) = parse_psn_lsn(&args)?;

        self.port.write_all(format!("00{psn:02X}0000").as_bytes())?;

        let mut data = [0; SECTOR_DATA_LEN];
        self.port.read_exact(&mut data)?;

        debug!("Data received");
        trace!("  data = {data:02x?}");

        let mut sector = &mut self.disk.sectors[psn as usize];
        sector.data = data;

        self.port.write_all(format!("00{psn:02X}0000").as_bytes())?;
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    fn fdc_read_sector(&mut self) -> Result<()> {
        let args = self.read_fdc_args()?;
        let (psn, _) = parse_psn_lsn(&args)?;

        self.port.write_all(format!("00{psn:02X}0000").as_bytes())?;

        let wait_value = read_single(&mut self.port)?;
        ensure!(wait_value == b'\r', "Expected \\r, got {wait_value:x}");

        let sector = &self.disk.sectors[psn as usize];
        self.port.write_all(&sector.data)?;

        Ok(())
    }

    fn read_fdc_args(&mut self) -> Result<Vec<Vec<u8>>> {
        let mut buf = vec![];

        loop {
            let arg = read_single(&mut self.port)?;
            if arg == b'\r' {
                break;
            } else if arg == b' ' {
                continue;
            }

            buf.push(arg);
        }

        let parsed_args = if buf.is_empty() {
            vec![]
        } else {
            buf.split(|b| *b == b',').map(|bs| bs.to_vec()).collect()
        };

        debug!("Raw FDC arguments {buf:02x?}, parsed args {parsed_args:02x?}");

        Ok(parsed_args)
    }
}

fn read_nonzero(port: &mut dyn Read, count: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(count);

    while buf.len() != count {
        let first_nonzero = buf.len();
        buf.resize(count, 0);
        port.read_exact(&mut buf[first_nonzero..])?;

        buf.retain(|b| *b != 0);
    }

    Ok(buf)
}

fn read_single(port: &mut dyn Read) -> Result<u8> {
    let mut buf = [0];
    port.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn parse_psn_lsn(args: &[Vec<u8>]) -> Result<(u8, u8)> {
    let mut psn = 0;
    let mut lsn = 1;

    if let Some(psn_arg_bytes) = args.get(0) {
        psn = std::str::from_utf8(psn_arg_bytes)?.parse::<u8>()?;
        ensure!(
            (psn as usize) < SECTOR_COUNT,
            "Sector index {psn} out of bounds"
        );
    }
    if let Some(lsn_arg_bytes) = args.get(1) {
        lsn = std::str::from_utf8(lsn_arg_bytes)?.parse::<u8>()?;
    }

    debug!("Parsed PSN={psn}, LSN={lsn}");

    Ok((psn, lsn))
}
