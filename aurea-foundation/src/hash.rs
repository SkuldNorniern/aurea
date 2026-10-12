//! A fast hasher for small keys that come from the app, not from outside:
//! widget ids, glyph keys, text being measured.
//!
//! The standard hasher resists keys chosen to collide, which costs more than
//! these maps need. This one mixes a word at a time, as the compiler's own
//! maps do.

use std::hash::{BuildHasherDefault, Hasher};

/// Multiplier spreading each word over the state.
const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

/// Hashes a word at a time. Not for keys an attacker picks.
#[derive(Clone, Copy, Debug, Default)]
pub struct FastHasher {
    hash: u64,
}

impl FastHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FastHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for word in words {
            self.add(u64::from_le_bytes(*word));
        }
        let mut tail = [0u8; 8];
        tail[..rest.len()].copy_from_slice(rest);
        // The length keeps "ab" + "" apart from "a" + "b".
        self.add(u64::from_le_bytes(tail) ^ ((rest.len() as u64) << 56));
    }

    #[inline]
    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u16(&mut self, n: u16) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    #[inline]
    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    #[inline]
    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

/// Builds [`FastHasher`]s, for `HashMap<K, V, FastBuild>`.
pub type FastBuild = BuildHasherDefault<FastHasher>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    fn hash(value: impl Hash) -> u64 {
        let mut h = FastHasher::default();
        value.hash(&mut h);
        h.finish()
    }

    #[test]
    fn nearby_keys_hash_apart() {
        assert_ne!(hash(1u32), hash(2u32));
        assert_ne!(hash("ab"), hash("ba"));
        assert_ne!(hash(("a", "b")), hash(("ab", "")));
        assert_eq!(hash("same text"), hash("same text"));
    }
}
