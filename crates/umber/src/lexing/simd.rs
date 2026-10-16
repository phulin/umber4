use super::hash::NameHash;
use super::scan::{self, Out, Scan};
use super::{Counts, Session, Source, Table};
use crate::token::Token;
use std::arch::x86_64::*;

/// Class bits for the source, 64 bytes per word, plus a final zero word so a
/// read at any text position can take its following word.
#[derive(Default)]
pub(super) struct Masks {
    /// Bytes that a character run writes as one token each.
    run: Vec<u64>,
    /// Category-11 bytes.
    letter: Vec<u64>,
    /// Run bytes other than letters, other characters, and blank spaces.
    special: Vec<u64>,
    /// Category-9 and category-10 bytes.
    spaces: Vec<u64>,
}

/// The 64 class bits starting at text position `p`.
#[inline(always)]
fn bits(words: &[u64], p: usize) -> u64 {
    let (word, offset) = (p / 64, p % 64);
    (words[word] >> offset) | ((words[word + 1] << 1) << (63 - offset))
}

/// The number of consecutive set bits from `p`, at most `limit`.
#[inline(always)]
fn ones(words: &[u64], p: usize, limit: usize) -> usize {
    let mut n = 0;
    while n < limit {
        let run = bits(words, p + n).trailing_ones() as usize;
        n += run;
        if run < 64 {
            break;
        }
    }
    n.min(limit)
}

/// Membership in an arbitrary byte set: the low nibble selects a table entry
/// holding one bit per high nibble, from one table for bytes below 128 and
/// one for the rest. A category table changes only these 32 bytes.
struct ByteSet {
    low: __m256i,
    high: __m256i,
}
impl ByteSet {
    #[target_feature(enable = "avx2")]
    fn new(member: impl Fn(u8) -> bool) -> Self {
        let mut tables = [[0u8; 16]; 2];
        for byte in 0..=255u8 {
            if member(byte) {
                tables[usize::from(byte >> 7)][usize::from(byte & 15)] |= 1 << ((byte >> 4) & 7);
            }
        }
        // SAFETY: each table has sixteen readable bytes.
        let [low, high] = tables.map(|table| unsafe { _mm_loadu_si128(table.as_ptr().cast()) });
        Self {
            low: _mm256_broadcastsi128_si256(low),
            high: _mm256_broadcastsi128_si256(high),
        }
    }
    #[target_feature(enable = "avx2")]
    fn contains(&self, bytes: __m256i, low: __m256i, bit: __m256i) -> u32 {
        let rows = _mm256_blendv_epi8(
            _mm256_shuffle_epi8(self.low, low),
            _mm256_shuffle_epi8(self.high, low),
            bytes,
        );
        _mm256_movemask_epi8(_mm256_cmpeq_epi8(_mm256_and_si256(rows, bit), bit)) as u32
    }
}

#[derive(Clone, Copy, Default)]
struct Block {
    /// Run bytes other than blank spaces.
    single: u64,
    letter: u64,
    special: u64,
    spaces: u64,
    blank: u64,
}

struct Classes {
    single: ByteSet,
    letter: ByteSet,
    special: ByteSet,
    spaces: ByteSet,
    blank: u64,
}
impl Classes {
    #[target_feature(enable = "avx2")]
    fn block(&self, bytes: &[u8; 64]) -> Block {
        let nibble = _mm256_set1_epi8(15);
        let bits = _mm256_setr_epi8(
            1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64, -128, 1, 2, 4, 8, 16, 32, 64,
            -128, 1, 2, 4, 8, 16, 32, 64, -128,
        );
        let space = _mm256_set1_epi8(32);
        let mut block = Block::default();
        for half in 0..2 {
            // SAFETY: each half is 32 readable bytes of the block.
            let v = unsafe { _mm256_loadu_si256(bytes[32 * half..].as_ptr().cast()) };
            let low = _mm256_and_si256(v, nibble);
            let bit =
                _mm256_shuffle_epi8(bits, _mm256_and_si256(_mm256_srli_epi16::<4>(v), nibble));
            let shift = 32 * half;
            block.single |= u64::from(self.single.contains(v, low, bit)) << shift;
            block.letter |= u64::from(self.letter.contains(v, low, bit)) << shift;
            block.special |= u64::from(self.special.contains(v, low, bit)) << shift;
            block.spaces |= u64::from(self.spaces.contains(v, low, bit)) << shift;
            block.blank |=
                u64::from(_mm256_movemask_epi8(_mm256_cmpeq_epi8(v, space)) as u32) << shift;
        }
        block.blank &= self.blank;
        block
    }
}

/// Stage 1: classify the whole source. A run byte is one with a run token,
/// or a blank space with one on each side.
#[target_feature(enable = "avx2")]
fn classify(source: &Source, table: &Table, tokens: &[u32; 256], masks: &mut Masks) {
    let cat = |byte: u8| table.cats[byte as usize];
    let classes = Classes {
        single: ByteSet::new(|byte| tokens[byte as usize] != 0),
        letter: ByteSet::new(|byte| cat(byte) == 11),
        special: ByteSet::new(|byte| tokens[byte as usize] != 0 && !matches!(cat(byte), 11 | 12)),
        spaces: ByteSet::new(|byte| matches!(cat(byte), 9 | 10)),
        blank: if table.cats[32] == 10 { u64::MAX } else { 0 },
    };
    let blocks = source.len().div_ceil(64);
    // The last block can extend into the padding, which PAD covers.
    let block = |index: usize| -> Block {
        classes.block(
            source.bytes[64 * index..64 * index + 64]
                .try_into()
                .unwrap(),
        )
    };
    for words in [
        &mut masks.run,
        &mut masks.letter,
        &mut masks.special,
        &mut masks.spaces,
    ] {
        words.clear();
        words.resize(blocks + 1, 0);
    }
    let mut previous = 0;
    let mut current = if blocks > 0 {
        block(0)
    } else {
        Block::default()
    };
    for index in 0..blocks {
        let next = if index + 1 < blocks {
            block(index + 1)
        } else {
            Block::default()
        };
        let before = (current.single << 1) | (previous >> 63);
        let after = (current.single >> 1) | (next.single << 63);
        masks.run[index] = current.single | (current.blank & before & after);
        masks.letter[index] = current.letter;
        masks.special[index] = current.special;
        masks.spaces[index] = current.spaces;
        previous = current.single;
        current = next;
    }
}

/// Runs shorter than this are written from the token table; a brace in
/// `\ExplSyntaxOn` code is usually a run of one.
const SHORT: usize = 8;

struct Avx2Scan<'a> {
    masks: &'a Masks,
    /// The token of each byte inside a run: its run token, or a space for a
    /// blank.
    tokens: &'a [u32; 256],
    /// The blank space byte when space has category 10, and otherwise a
    /// value no widened byte equals.
    blank: i32,
}
impl Scan for Avx2Scan<'_> {
    #[inline(always)]
    fn run(&self, text: &[u8], p: usize, limit: usize, out: &mut Out) -> usize {
        let n = ones(&self.masks.run, p, limit);
        if n < SHORT {
            for &byte in &text[p..p + n] {
                out.push(self.tokens[byte as usize]);
            }
        } else {
            // SAFETY: Avx2Scan is built only by lex, which requires AVX2. The
            // run lies within the text, so its tokens fit the output bound.
            unsafe { emit(text, p, n, self, out) };
        }
        n
    }
    #[inline(always)]
    fn letters(&self, _: &[u8], p: usize, limit: usize) -> usize {
        ones(&self.masks.letter, p, limit)
    }
    #[inline(always)]
    fn spaces(&self, _: &[u8], p: usize, limit: usize) -> usize {
        ones(&self.masks.spaces, p, limit)
    }
}

/// Writes the run `text[p..p + n]` eight tokens at a time, then patches
/// its special bytes from the token table. A vector token is the
/// zero-extended byte with category 11, 12, or 10 in its second byte.
///
/// # Safety
///
/// The processor supports AVX2, `p + n` is at most the text length, and the
/// output can take `n` more tokens within its bound.
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn emit(text: &[u8], p: usize, n: usize, scan: &Avx2Scan, out: &mut Out) {
    let lanes = _mm256_setr_epi32(1, 2, 4, 8, 16, 32, 64, 128);
    let other = _mm256_set1_epi32(12 << 8);
    let to_letter = _mm256_set1_epi32((12 ^ 11) << 8);
    let to_blank = _mm256_set1_epi32((12 ^ 10) << 8);
    let blank = _mm256_set1_epi32(scan.blank);
    let tail = out.tail();
    let mut i = 0;
    while i < n {
        let start = i;
        let mut letter = bits(&scan.masks.letter, p + i);
        let mut special = bits(&scan.masks.special, p + i);
        if n - i < 64 {
            special &= (1 << (n - i)) - 1;
        }
        let end = n.min(i + 64);
        while i < end {
            // SAFETY: p + i is inside the text and PAD bytes follow it, so
            // eight bytes are readable. Cells up to SLACK past the bound are
            // writable.
            let chars =
                _mm256_cvtepu8_epi32(unsafe { _mm_loadl_epi64(text.as_ptr().add(p + i).cast()) });
            let letters = _mm256_set1_epi32((letter & 255) as i32);
            let letters = _mm256_cmpeq_epi32(_mm256_and_si256(letters, lanes), lanes);
            let blanks = _mm256_cmpeq_epi32(chars, blank);
            let cats = _mm256_xor_si256(
                other,
                _mm256_or_si256(
                    _mm256_and_si256(letters, to_letter),
                    _mm256_and_si256(blanks, to_blank),
                ),
            );
            unsafe { _mm256_storeu_si256(tail.add(i).cast(), _mm256_or_si256(chars, cats)) };
            letter >>= 8;
            i += 8;
        }
        while special != 0 {
            let k = start + special.trailing_zeros() as usize;
            // SAFETY: k is below n, inside the cells this run publishes.
            unsafe { tail.add(k).write(scan.tokens[text[p + k] as usize]) };
            special &= special - 1;
        }
    }
    out.advance(n);
}

/// Classifies the source, then lexes it with mask-driven runs.
#[target_feature(enable = "avx2")]
pub(super) fn lex<H: NameHash>(
    source: &Source,
    table: &Table,
    session: &mut Session,
    out: &mut Vec<u32>,
) -> Counts {
    let mut masks = std::mem::take(&mut session.masks);
    let mut tokens = scan::run_tokens(&table.cats);
    classify(source, table, &tokens, &mut masks);
    if table.cats[32] == 10 {
        tokens[32] = Token::SPACE.0;
    }
    let scan = Avx2Scan {
        masks: &masks,
        tokens: &tokens,
        blank: if table.cats[32] == 10 { 32 } else { 256 },
    };
    let counts = scan::lex::<_, H>(&scan, source, table, session, out);
    session.masks = masks;
    counts
}
