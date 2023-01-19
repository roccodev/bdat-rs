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

pub fn murmur3(bytes: &[u8]) -> u32 {
    let len = bytes.len() as u32;

    let mut hash = MURMUR3_SEED;
    let mut buf = [0u8; 4];
    let mut start = 0;
    for _ in 0..(len as usize >> 2) {
        buf.copy_from_slice(&bytes[start..(start + 4)]);
        hash ^= murmur3_scramble(u32::from_le_bytes(buf));
        hash = (hash.wrapping_shl(13)) | (hash.wrapping_shr(19));
        hash = hash.wrapping_mul(5).wrapping_add(0xe6546b64);
        start += 4;
    }

    let mut k = 0;
    for i in (0..(len as usize & 3)).rev() {
        k <<= 8;
        k |= bytes[start + i] as u32;
    }

    hash ^= murmur3_scramble(k);
    hash ^= len;
    hash ^= hash.wrapping_shr(16);
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash.wrapping_shr(13);
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash ^= hash.wrapping_shr(16);
    hash
}

#[inline]
pub fn murmur3_str(src: &str) -> u32 {
    murmur3(src.as_bytes())
}

#[inline]
fn murmur3_scramble(mut k: u32) -> u32 {
    k = k.wrapping_mul(0xcc9e2d51);
    k = (k.wrapping_shl(15)) | (k.wrapping_shr(17));
    k = k.wrapping_mul(0x1b873593);
    k
}

#[cfg(test)]
mod tests {
    use super::murmur3_str;

    #[test]
    fn test_murmur3() {
        assert_eq!(murmur3_str("FLD_EnemyData"), 0x2521C473);
        assert_eq!(murmur3_str("EVT_listEv"), 0x23EE284B);
    }
}
