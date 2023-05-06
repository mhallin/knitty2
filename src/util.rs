#![allow(dead_code)] // FIXME remove this

use crate::Nibble;

/// Convert a stream of 4 bit numbers to a stream of bits
pub fn nibble_bits(ns: &[Nibble]) -> Vec<bool> {
    let mut bits = vec![false; ns.len() * 4];

    for (src, dest) in ns.iter().copied().zip(bits.chunks_exact_mut(4)) {
        let src: u8 = src.into();
        dest[0] = (src & 8) >> 3 != 0;
        dest[1] = (src & 4) >> 2 != 0;
        dest[2] = (src & 2) >> 1 != 0;
        dest[3] = (src & 1) != 0;
    }

    bits
}

#[test]
fn test_nibble_bits() {
    assert_eq!(
        nibble_bits(&[Nibble::new(1), Nibble::new(2)]),
        &[false, false, false, true, false, false, true, false]
    );
}

/// Convert a string of bytes to a stream of nibbles
pub fn to_nibbles(bs: &[u8]) -> Vec<Nibble> {
    let mut ns = vec![Nibble::ZERO; bs.len() * 2];

    for (src, dest) in bs.iter().copied().zip(ns.chunks_exact_mut(2)) {
        let (n1, n2) = Nibble::divide_byte(src);
        dest[0] = n1;
        dest[1] = n2;
    }

    ns
}

#[test]
fn test_to_nibbles() {
    assert_eq!(to_nibbles(&[0x3d]), &[Nibble::new(3), Nibble::new(13)]);
}

/// Convert a stream of nibbles to a string of bytes
pub fn from_nibbles(ns: &[Nibble]) -> Vec<u8> {
    assert_eq!(ns.len() % 2, 0, "Must provide an even number of nibbles");

    let mut bs = vec![0; ns.len() / 2];

    for (src, dest) in ns.chunks_exact(2).zip(bs.iter_mut()) {
        *dest = Nibble::combine_nibbles(src[0], src[1]);
    }

    bs
}

#[test]
fn test_from_nibbles() {
    assert_eq!(from_nibbles(&[Nibble::new(3), Nibble::new(13)]), &[0x3d]);
}

/// Convert a stream of nibbles representing a BCD (binary coded digit) to an integer
pub fn from_bcd(ns: &[Nibble]) -> u16 {
    let mut s = 0;
    let mut m = 1;

    for n in ns.iter().copied().rev() {
        let n: u8 = n.into();
        s += u16::from(n) * m;
        m *= 10;
    }

    s
}

#[test]
fn test_from_bcd() {
    assert_eq!(
        from_bcd(&[Nibble::new(1), Nibble::new(2), Nibble::new(3)]),
        123
    );
}

/// Convert an integer to a list of nibbles representing the number in BCD
///
/// Optionally pads the number with initial zeroes to a specified width.
pub fn to_bcd(mut n: u16, min_width: u16) -> Vec<Nibble> {
    let mut ns = vec![];

    while n != 0 {
        ns.push(Nibble::new((n % 10) as u8));
        n /= 10;
    }

    while ns.len() < usize::from(min_width) {
        ns.push(Nibble::ZERO);
    }

    ns.reverse();
    ns
}

#[test]
fn test_to_bcd() {
    assert_eq!(
        to_bcd(123, 0),
        &[Nibble::new(1), Nibble::new(2), Nibble::new(3)]
    );
    assert_eq!(
        to_bcd(12, 5),
        &[
            Nibble::ZERO,
            Nibble::ZERO,
            Nibble::ZERO,
            Nibble::new(1),
            Nibble::new(2),
        ]
    );
}

/// Convert a sequence of bits to a string of bytes
///
/// The bit sequence must have a length divisible by 8
pub fn bits_to_bytes(bits: &[bool]) -> Vec<u8> {
    assert_eq!(
        bits.len() % 8,
        0,
        "Must have a length divisible by 8, got {}",
        bits.len()
    );

    let mut bs = vec![0; bits.len() / 8];

    for (src, dest) in bits.chunks_exact(8).zip(bs.iter_mut()) {
        let mut s = 0;
        let mut c = 128;
        for b in src.iter().copied() {
            if b {
                s += c;
            }
            c /= 2;
        }

        *dest = s;
    }

    bs
}

#[test]
fn test_bits_to_bytes() {
    assert_eq!(
        bits_to_bytes(&[false, false, true, false, false, true, false, true]),
        &[0x25]
    );
}

pub fn padding<T>(n: T, alignment: T) -> T
where
    T: std::ops::Rem<T, Output = T>,
    T: Copy,
    T: std::ops::Sub<T, Output = T>,
{
    (alignment - (n % alignment)) % alignment
}

#[test]
fn test_padding() {
    assert_eq!(padding(3, 4), 1);
    assert_eq!(padding(4, 4), 0);
}
