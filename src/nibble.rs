#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Nibble(u8);

impl Nibble {
    pub const ZERO: Nibble = Nibble(0);

    pub fn new(v: u8) -> Nibble {
        assert!(v <= 0xf, "Too many bits in nibble: {v:x}");
        Nibble(v)
    }

    pub fn divide_byte(v: u8) -> (Nibble, Nibble) {
        (Nibble::new(v >> 4), Nibble(v & 0xf))
    }

    pub fn combine_nibbles(n1: Nibble, n2: Nibble) -> u8 {
        (n1.0 << 4) | n2.0
    }
}

impl From<Nibble> for u8 {
    fn from(value: Nibble) -> Self {
        value.0
    }
}

#[test]
fn divide_byte() {
    let byte = 0x3d;
    let (n1, n2) = Nibble::divide_byte(byte);

    assert_eq!(n1, Nibble::new(0x3));
    assert_eq!(n2, Nibble::new(0xd));
}

#[test]
fn combine_nibbles() {
    let n1 = Nibble::new(0x3);
    let n2 = Nibble::new(0xd);

    let byte = Nibble::combine_nibbles(n1, n2);
    assert_eq!(byte, 0x3d);
}