//! Whole-file lexing prototype for `docs/fast-lexing.md`.
//!
//! Each lexer reads a complete file under one fixed category table and
//! `\endlinechar`, producing exactly the tokens that `FileInput::read` returns
//! when nothing changes those settings. The engine does not use this module;
//! the `lexing` example compares its variants with the current reader.

mod hash;
mod line;
mod scan;
#[cfg(target_arch = "x86_64")]
mod simd;
#[cfg(test)]
mod tests;

use crate::Error;
use crate::input::FileInput;
use crate::names::Names;
use crate::token::{Integer, Token};
use hash::{Fnv, NameHash, Word};

/// Stands in for the current reader's invalid-character error, so a
/// comparison can continue past it.
pub const INVALID: u32 = u32::MAX;
/// Initialized bytes after the text, so word and vector loads that start
/// inside it stay inside the allocation.
pub const PAD: usize = 64;

/// Immutable source bytes followed by `PAD` zero bytes. A host adapter can
/// allocate its resource buffers this way without a separate copy.
pub struct Source {
    bytes: Vec<u8>,
}
impl Source {
    pub fn new(text: &[u8]) -> Self {
        let mut bytes = Vec::with_capacity(text.len() + PAD);
        bytes.extend_from_slice(text);
        bytes.resize(text.len() + PAD, 0);
        Self { bytes }
    }
    pub fn len(&self) -> usize {
        self.bytes.len() - PAD
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// A fixed category table and end-of-line character.
#[derive(Clone)]
pub struct Table {
    pub cats: [u8; 256],
    pub endline: i32,
}
impl Table {
    /// INITEX's initial codes.
    pub fn initex() -> Self {
        let mut cats = [12; 256];
        for byte in (b'a'..=b'z').chain(b'A'..=b'Z') {
            cats[byte as usize] = 11;
        }
        for (byte, cat) in [
            (b'\\', 0),
            (b' ', 10),
            (13, 5),
            (b'%', 14),
            (0, 9),
            (127, 15),
        ] {
            cats[byte as usize] = cat;
        }
        Self { cats, endline: 13 }
    }
    /// The document codes set by plain.tex.
    pub fn plain() -> Self {
        let mut table = Self::initex();
        for (byte, cat) in [
            (b'{', 1),
            (b'}', 2),
            (b'$', 3),
            (b'&', 4),
            (b'#', 6),
            (b'^', 7),
            (11, 7),
            (b'_', 8),
            (1, 8),
            (9, 10),
            (b'~', 13),
            (12, 13),
        ] {
            table.cats[byte as usize] = cat;
        }
        table
    }
    /// Package code: plain's codes with `@` as a letter.
    pub fn package() -> Self {
        let mut table = Self::plain();
        table.cats[b'@' as usize] = 11;
        table
    }
    /// `\ExplSyntaxOn`: blanks are ignored, `_` and `:` are letters, `~` is a
    /// space, and the end-of-line character is an ignored blank.
    pub fn expl3() -> Self {
        let mut table = Self::package();
        for (byte, cat) in [
            (9, 9),
            (b' ', 9),
            (b'"', 12),
            (b':', 11),
            (b'_', 11),
            (b'|', 12),
            (b'~', 10),
        ] {
            table.cats[byte as usize] = cat;
        }
        table.endline = 32;
        table
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lexer {
    /// `FileInput::read`, one token per call.
    Current,
    /// Whole-file byte loops with FNV-1a, the current name hash.
    ScalarFnv,
    /// Whole-file byte loops with the word-at-a-time name hash.
    Scalar,
    /// AVX2 classification and run writes with FNV-1a.
    Avx2Fnv,
    /// AVX2 classification and run writes with the word-at-a-time name hash.
    Avx2,
}
impl Lexer {
    pub const ALL: [Self; 5] = [
        Self::Current,
        Self::ScalarFnv,
        Self::Scalar,
        Self::Avx2Fnv,
        Self::Avx2,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::ScalarFnv => "scalar-fnv",
            Self::Scalar => "scalar",
            Self::Avx2Fnv => "avx2-fnv",
            Self::Avx2 => "avx2",
        }
    }
    pub fn available(self) -> bool {
        match self {
            #[cfg(target_arch = "x86_64")]
            Self::Avx2Fnv | Self::Avx2 => std::arch::is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))]
            Self::Avx2Fnv | Self::Avx2 => false,
            _ => true,
        }
    }
    fn word_hash(self) -> bool {
        matches!(self, Self::Scalar | Self::Avx2)
    }
}

/// A name table, its `\par` identity, and reusable lexer scratch. A table is
/// probed with one hash, so a session serves only lexers that share it.
pub struct Session {
    names: Names,
    par: Token,
    word_hash: bool,
    line: Vec<u8>,
    #[cfg(target_arch = "x86_64")]
    masks: simd::Masks,
}
impl Session {
    pub fn new(lexer: Lexer) -> Self {
        let word_hash = lexer.word_hash();
        let (names, par) = Self::fresh_names(word_hash);
        Self {
            names,
            par,
            word_hash,
            line: Vec::new(),
            #[cfg(target_arch = "x86_64")]
            masks: simd::Masks::default(),
        }
    }
    fn fresh_names(word_hash: bool) -> (Names, Token) {
        let mut names = Names::new();
        let hash = if word_hash {
            Word::exact(b"par")
        } else {
            Fnv::exact(b"par")
        };
        let par = names.intern_hashed(b"par", hash).unwrap();
        (names, Token::cs(par))
    }
    /// Replaces the name table with an empty one, keeping lexer scratch.
    pub fn forget_names(&mut self) {
        (self.names, self.par) = Self::fresh_names(self.word_hash);
    }
    /// The number of control-sequence identities, including fixed ones.
    pub fn names(&self) -> usize {
        self.names.len()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub lines: usize,
    /// Lines finished by the prepared-line reader after a `^^` pair or a
    /// letter end-of-line character.
    pub fallback_lines: usize,
}

/// Appends the file's tokens to `out`. The current reader reports no counts.
pub fn lex(
    lexer: Lexer,
    source: &Source,
    table: &Table,
    session: &mut Session,
    out: &mut Vec<u32>,
) -> Counts {
    assert_eq!(
        session.word_hash,
        lexer.word_hash(),
        "a session's names use one hash"
    );
    assert!(
        lexer.available(),
        "{} is unavailable on this processor",
        lexer.name()
    );
    match lexer {
        Lexer::Current => {
            current(source, table, session, out);
            Counts::default()
        }
        Lexer::ScalarFnv => {
            scan::lex::<_, Fnv>(&scan::Bytes::new(table), source, table, session, out)
        }
        Lexer::Scalar => {
            scan::lex::<_, Word>(&scan::Bytes::new(table), source, table, session, out)
        }
        // SAFETY: available() detected AVX2 above.
        #[cfg(target_arch = "x86_64")]
        Lexer::Avx2Fnv => unsafe { simd::lex::<Fnv>(source, table, session, out) },
        // SAFETY: available() detected AVX2 above.
        #[cfg(target_arch = "x86_64")]
        Lexer::Avx2 => unsafe { simd::lex::<Word>(source, table, session, out) },
        #[cfg(not(target_arch = "x86_64"))]
        Lexer::Avx2Fnv | Lexer::Avx2 => unreachable!(),
    }
}

fn current(source: &Source, table: &Table, session: &mut Session, out: &mut Vec<u32>) {
    let cats = table.cats.map(|cat| Integer {
        value: i32::from(cat),
        level: 0,
    });
    let mut file = FileInput::new(&source.bytes[..source.len()]);
    loop {
        match file.read(&cats, table.endline, &mut session.names, session.par) {
            Ok(token) => out.push(token.0),
            Err(Error::Syntax) => out.push(INVALID),
            Err(Error::Eof) => return,
            Err(error) => panic!("current reader failed: {error}"),
        }
    }
}
