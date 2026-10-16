use crate::names::Names;

/// A name hash for multi-byte control words. `padded` reads a name in place
/// and may load bytes after it; it must equal `exact` on the same spelling.
pub(super) trait NameHash {
    fn padded(text: &[u8], start: usize, len: usize) -> u64;
    fn exact(name: &[u8]) -> u64;
}

/// FNV-1a, the reader's current byte-serial hash.
pub(super) struct Fnv;
impl NameHash for Fnv {
    #[inline(always)]
    fn padded(text: &[u8], start: usize, len: usize) -> u64 {
        Self::exact(&text[start..start + len])
    }
    #[inline(always)]
    fn exact(name: &[u8]) -> u64 {
        name.iter().fold(Names::HASH_START, |hash, &byte| {
            Names::hash_byte(hash, byte)
        })
    }
}

/// Folds one or two little-endian words with a 64x64->128-bit multiply.
/// Names up to sixteen bytes take two loads and one multiply instead of a
/// dependent multiply per byte.
pub(super) struct Word;

const K0: u64 = 0xa076_1d64_78bd_642f;
const K1: u64 = 0xe703_7ed1_a0b4_28db;
const K2: u64 = 0x8ebc_6af0_9c88_c6e3;

#[inline(always)]
fn fold(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    product as u64 ^ (product >> 64) as u64
}
#[inline(always)]
fn word(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}
impl Word {
    #[inline(always)]
    fn finish(a: u64, b: u64, len: usize) -> u64 {
        fold(a ^ K0, b ^ K1 ^ len as u64)
    }
    #[inline(never)]
    fn long(name: &[u8]) -> u64 {
        let len = name.len();
        let mut seed = K2;
        let mut at = 0;
        while at + 16 < len {
            seed = fold(word(name, at) ^ K1, word(name, at + 8) ^ seed);
            at += 16;
        }
        Self::finish(word(name, len - 16) ^ seed, word(name, len - 8), len)
    }
}
impl NameHash for Word {
    #[inline(always)]
    fn padded(text: &[u8], start: usize, len: usize) -> u64 {
        debug_assert!(len >= 2 && start + len.max(8) <= text.len());
        if len <= 8 {
            Self::finish(word(text, start) & (u64::MAX >> (64 - 8 * len)), 0, len)
        } else if len <= 16 {
            Self::finish(word(text, start), word(text, start + len - 8), len)
        } else {
            Self::long(&text[start..start + len])
        }
    }
    fn exact(name: &[u8]) -> u64 {
        let len = name.len();
        if len <= 8 {
            let mut first = [0; 8];
            first[..len].copy_from_slice(name);
            Self::finish(u64::from_le_bytes(first), 0, len)
        } else if len <= 16 {
            Self::finish(word(name, 0), word(name, len - 8), len)
        } else {
            Self::long(name)
        }
    }
}
