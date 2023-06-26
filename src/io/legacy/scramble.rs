#[derive(Ord, PartialOrd, Eq, PartialEq, Debug)]
pub enum ScrambleType {
    None,
    Scrambled(u16),
}

/// Unscrambles a section of legacy BDAT data.
#[inline]
pub fn unscramble(data: &mut [u8], key: u16) {
    unscramble_chunks(data, key)
}

pub fn calc_checksum(full_table: &[u8]) -> u32 {
    0
}

// Various unscramble implementations - all correct (unit-tested below) and benchmarked
// (see benches/scramble.rs)

#[cfg(any(test, feature = "bench"))]
#[inline]
pub fn unscramble_naive(data: &mut [u8], key: u16) {
    let mut t1 = ((!key >> 8) & 0xff) as u8;
    let mut t2 = (!key & 0xff) as u8;
    let mut i = 0;
    while i < data.len() {
        let a = data[i];
        let b = data[i + 1];
        data[i] ^= t1;
        data[i + 1] ^= t2;
        t1 = t1.wrapping_add(a);
        t2 = t2.wrapping_add(b);
        i += 2;
    }
}

//#[cfg(any(test, feature = "bench"))]
#[inline]
pub fn unscramble_chunks(data: &mut [u8], key: u16) {
    let mut t1 = ((key >> 8) ^ 0xff) as u8;
    let mut t2 = (key ^ 0xff) as u8;
    for x in data.chunks_exact_mut(2) {
        let [a, b, ..] = x else { unreachable!() };
        let old_a = *a;
        let old_b = *b;
        *a ^= t1;
        *b ^= t2;
        t1 = t1.wrapping_add(old_a);
        t2 = t2.wrapping_add(old_b);
    }
}

#[cfg(any(test, feature = "bench"))]
#[inline(never)] // worse performance
pub fn unscramble_single(data: &mut [u8], key: u16) {
    let mut t1 = ((key >> 8) ^ 0xff) as u8;
    let mut t2 = (key ^ 0xff) as u8;
    let mut key = &mut t1;
    let mut b = false;
    for x in data {
        let old = *x;
        *x ^= *key;
        *key = key.wrapping_add(old);
        key = if b { &mut t1 } else { &mut t2 };
        b = !b;
    }
}

#[cfg(any(test, feature = "bench"))]
pub mod tests {
    pub const INPUT: [u8; 14] = [
        0xfb, 0x7e, 0xe4, 0xf1, 0xe4, 0xeb, 0x4b, 0xba, 0xf4, 0x75, 0xe7, 0xd4, 0xec, 0x8d,
    ];

    // "MNU_qt2001_ms\0"
    const EXPECTED: [u8; 14] = [
        0x4d, 0x4e, 0x55, 0x5f, 0x71, 0x74, 0x32, 0x30, 0x30, 0x31, 0x5f, 0x6d, 0x73, 0x00,
    ];

    pub const KEY: u16 = 0x49cf;

    #[test]
    fn naive() {
        assert(super::unscramble_naive);
    }

    #[test]
    fn chunks() {
        assert(super::unscramble_chunks);
    }

    #[test]
    fn single() {
        assert(super::unscramble_single);
    }

    fn assert(f: fn(&mut [u8], u16)) {
        let mut data = INPUT;
        f(&mut data, KEY);
        assert_eq!(data, EXPECTED);
    }
}
