//! Hash utilities (+ a murmur3 implementation) for XC3 BDATs

const MURMUR3_SEED: u32 = 0;

#[cfg(feature = "hash-table")]
pub use table::{IdentityHasher, PreHashedMap};

#[cfg(feature = "hash-table")]
mod table {
    use std::hash::{BuildHasher, Hasher};

    /// A [`Hasher`] implementation for pre-hashed keys.
    #[derive(Clone, Copy, Default)]
    pub struct IdentityHasher(u64);

    pub type PreHashedMap<K, V> = std::collections::HashMap<K, V, IdentityHasher>;

    impl BuildHasher for IdentityHasher {
        type Hasher = Self;

        fn build_hasher(&self) -> Self::Hasher {
            *self
        }
    }

    impl Hasher for IdentityHasher {
        fn finish(&self) -> u64 {
            self.0
        }

        fn write(&mut self, bytes: &[u8]) {
            let mut int = [0u8; 4];
            int.copy_from_slice(bytes);

            let int = u32::from_le_bytes(int) as u64;
            self.0 = int | (int << 31);
        }
    }
}

/// Creates a murmur3-hashed [`Label`] from an expression.
///
/// ## Behavior
/// * If a string literal is passed in, the result will be `const`-evaluated.
/// * If an expression is passed in, the value is hashed and stored in the label. The expression's
/// value must implement `Borrow<str>`.
///
/// [`Label`]: crate::Label
#[macro_export]
macro_rules! label_hash {
    ($text:literal) => {{
        // const evaluation for string literals
        const HASH: $crate::Label = $crate::Label::Hash($crate::hash::murmur3_str($text));
        HASH
    }};
    ($text:expr) => {{
        let text: &dyn ::std::borrow::Borrow<str> = &$text;
        $crate::Label::Hash($crate::hash::murmur3_str(text.borrow()))
    }};
}

// MIT-licensed const version of murmur3, adapted from
// https://github.com/Reboare/const-murmur3
pub const fn murmur3(data: &[u8]) -> u32 {
    murmur3_with_seed(data, MURMUR3_SEED)
}

pub const fn murmur3_with_seed(data: &[u8], seed: u32) -> u32 {
    let slice_size: usize = data.len();
    let mut hash = seed;
    let mut i = 0;
    let iterator = slice_size / 4;
    while i < iterator {
        // Relax the bounds-checker
        assert!(data.len() > i * 4 + 3);
        let data = [
            data[i * 4],
            data[i * 4 + 1],
            data[i * 4 + 2],
            data[i * 4 + 3],
        ];
        hash ^= murmur3_scramble(data);
        hash = (hash.wrapping_shl(13)) | (hash.wrapping_shr(19));
        hash = hash.wrapping_mul(5).wrapping_add(0xe6546b64);

        i += 1;
    }
    match slice_size % 4 {
        0 => (),
        1 => {
            let data = [data[i * 4], 0, 0, 0];
            let k = murmur3_scramble(data);
            hash ^= k;
        }
        2 => {
            let data = [data[i * 4], data[i * 4 + 1], 0, 0];
            let k = murmur3_scramble(data);
            hash ^= k;
        }
        3 => {
            let data = [data[i * 4], data[i * 4 + 1], data[i * 4 + 2], 0];
            let k = murmur3_scramble(data);
            hash ^= k;
        }
        _ => unreachable!(),
    }

    hash ^= slice_size as u32;
    hash = hash ^ (hash.wrapping_shr(16));
    hash = hash.wrapping_mul(0x85ebca6b);
    hash = hash ^ (hash.wrapping_shr(13));
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash = hash ^ (hash.wrapping_shr(16));

    hash
}

#[inline]
pub const fn murmur3_str(src: &str) -> u32 {
    murmur3(src.as_bytes())
}

#[inline]
const fn murmur3_scramble(data: [u8; 4]) -> u32 {
    let r1 = 15;
    let c1: u32 = 0xcc9e2d51;
    let c2: u32 = 0x1b873593;
    let mut k = u32::from_le_bytes(data);
    k = k.wrapping_mul(c1);
    k = k.rotate_left(r1);
    k = k.wrapping_mul(c2);
    k
}

#[cfg(test)]
mod tests {
    use super::murmur3_str;

    #[test]
    fn test_murmur3() {
        assert_eq!(murmur3_str("abc"), 0xB3DD93FA);
        assert_eq!(murmur3_str("FLD_EnemyData"), 0x2521C473);
        assert_eq!(murmur3_str("EVT_listEv"), 0x23EE284B);
    }
}
