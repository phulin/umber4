use crate::{Error, Result};
use std::cell::UnsafeCell;
use std::rc::Rc;

pub(crate) const CS: u32 = 1 << 24;
pub(crate) const PARAM: u32 = 1 << 23;
pub(crate) const DEFS: u32 = u32::MAX;
pub(crate) const SINGLE: u32 = DEFS - 1;
pub(crate) const FROZEN_RELAX: Token = Token(CS);
pub(crate) const CHUNK_BITS: u32 = 12;
const CHUNK_LEN: usize = 1 << CHUNK_BITS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Token(pub u32);
impl Token {
    pub fn character(cat: u8, byte: u8) -> Self {
        Self((u32::from(cat) << 8) | u32::from(byte))
    }
    pub fn cs(id: u32) -> Self {
        Self(CS | id)
    }
    pub fn is_cs(self) -> bool {
        self.0 >= CS
    }
    pub fn id(self) -> usize {
        (self.0 - CS) as usize
    }
    pub fn cat(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn byte(self) -> u8 {
        self.0 as u8
    }
    pub fn is_begin(self) -> bool {
        self.0 >> 8 == 1
    }
    pub fn is_end(self) -> bool {
        self.0 >> 8 == 2
    }
    pub fn is_space(self) -> bool {
        self.0 == (10 << 8 | 32)
    }
    pub fn other(byte: u8) -> Self {
        Self::character(12, byte)
    }
}
pub(crate) mod cmd {
    pub const BEGIN: u8 = 1;
    pub const END_GROUP_CHAR: u8 = 2;
    pub const SPACE: u8 = 10;
    pub const RELAX: u8 = 16;
    pub const DEF: u8 = 17;
    pub const GDEF: u8 = 18;
    pub const EDEF: u8 = 19;
    pub const XDEF: u8 = 20;
    pub const LET: u8 = 21;
    pub const GLOBAL: u8 = 22;
    pub const LONG: u8 = 23;
    pub const BEGIN_GROUP: u8 = 24;
    pub const END_GROUP: u8 = 25;
    pub const COUNT: u8 = 26;
    pub const COUNT_DEF: u8 = 27;
    pub const ARITH: u8 = 28;
    pub const CATCODE: u8 = 29;
    pub const ENDLINE: u8 = 30;
    pub const MESSAGE: u8 = 31;
    pub const COUNT_REF: u8 = 32;
    pub const ENDCSNAME: u8 = 33;
    pub const END: u8 = 34;
    pub const PAR: u8 = 35;
    pub const TOKS: u8 = 36;
    pub const TOKS_REF: u8 = 37;
    pub const CHAR_GIVEN: u8 = 38;
    pub const CHAR_DEF: u8 = 39;
    pub const FUTURELET: u8 = 40;
    pub const TOKS_DEF: u8 = 41;
    pub const AFTER_ASSIGN: u8 = 42;
    pub const AFTER_GROUP: u8 = 43;
    pub const INTEGER_PARAM: u8 = 44;
    pub const CASE_CODE: u8 = 45;
    pub const CHANGE_CASE: u8 = 46;
    pub const PROTECTED: u8 = 47;
    pub const NUMEXPR: u8 = 48;
    pub const MACRO: u8 = 64;
    pub const PROTECTED_MACRO: u8 = 65;
    pub const UNDEFINED: u8 = 66;
    pub const NUMBER: u8 = 67;
    pub const THE: u8 = 68;
    pub const EXPANDAFTER: u8 = 69;
    pub const NOEXPAND: u8 = 70;
    pub const CSNAME: u8 = 71;
    pub const IFNUM: u8 = 72;
    pub const IFX: u8 = 73;
    pub const IFTRUE: u8 = 74;
    pub const IFFALSE: u8 = 75;
    pub const ELSE: u8 = 76;
    pub const FI: u8 = 77;
    pub const STRING: u8 = 78;
    pub const ROMAN: u8 = 79;
    pub const MEANING: u8 = 80;
    pub const IF: u8 = 81;
    pub const IFCAT: u8 = 82;
    pub const IFODD: u8 = 83;
    pub const IFCASE: u8 = 84;
    pub const OR: u8 = 85;
    pub const IFDEFINED: u8 = 86;
    pub const IFCSNAME: u8 = 87;
    pub const UNEXPANDED: u8 = 88;
    pub const DETOKENIZE: u8 = 89;
    pub const UNLESS: u8 = 90;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Meaning {
    pub command: u8,
    pub value: i32,
}
impl Meaning {
    pub const UNDEFINED: Self = Self {
        command: cmd::UNDEFINED,
        value: 0,
    };
    pub const RELAX: Self = Self {
        command: cmd::RELAX,
        value: 0,
    };
    pub fn is_macro(self) -> bool {
        matches!(self.command, cmd::MACRO | cmd::PROTECTED_MACRO)
    }
    pub fn is_conditional(self) -> bool {
        matches!(
            self.command,
            cmd::IFNUM
                | cmd::IFX
                | cmd::IFTRUE
                | cmd::IFFALSE
                | cmd::IF
                | cmd::IFCAT
                | cmd::IFODD
                | cmd::IFCASE
                | cmd::IFDEFINED
                | cmd::IFCSNAME
        )
    }
    pub fn expandable(self) -> bool {
        self.command >= cmd::MACRO
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Binding {
    pub value: i32,
    pub level: u16,
    pub command: u8,
}
impl Binding {
    pub fn new(meaning: Meaning, level: u16) -> Self {
        Self {
            value: meaning.value,
            command: meaning.command,
            level,
        }
    }
    pub fn meaning(self) -> Meaning {
        Meaning {
            command: self.command,
            value: self.value,
        }
    }
}
impl Default for Binding {
    fn default() -> Self {
        Self::new(Meaning::UNDEFINED, 0)
    }
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Integer {
    pub value: i32,
    pub level: u16,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    // SINGLE encodes an inline token in start with end = start + 1. It is
    // confined to a one-token sequence and promoted before another range joins.
    pub store: u32,
    pub start: u32,
    pub end: u32,
}
impl Span {
    pub fn empty(self) -> bool {
        self.start == self.end
    }
}
#[derive(Default)]
pub(crate) struct Sequence {
    pub first: Span,
    pub rest: Vec<Span>,
}
impl Sequence {
    #[inline(always)]
    pub fn append(&mut self, span: Span) {
        if span.empty() {
            return;
        }
        let last = self.rest.last_mut().unwrap_or(&mut self.first);
        if last.empty() {
            *last = span;
        } else if last.store == span.store && last.end == span.start {
            last.end = span.end;
        } else {
            self.rest.push(span);
        }
    }
    pub fn span(&self, index: usize) -> Option<Span> {
        if index == 0 {
            (!self.first.empty()).then_some(self.first)
        } else {
            self.rest.get(index - 1).copied()
        }
    }
    pub fn trim_front(&mut self) {
        self.first.start += 1;
        if self.first.empty() && !self.rest.is_empty() {
            self.first = self.rest.remove(0);
        }
    }
    pub fn trim_back(&mut self, mut count: usize) {
        while count > 0 {
            let last = self.rest.last_mut().unwrap_or(&mut self.first);
            let n = count.min((last.end - last.start) as usize);
            last.end -= n as u32;
            count -= n;
            if last.empty() && !self.rest.is_empty() {
                self.rest.pop();
            }
        }
    }
    pub fn clear(&mut self) {
        self.first = Span::default();
        self.rest.clear();
    }
}
#[derive(Clone, Copy, Default)]
pub(crate) struct TokenValue {
    pub span: Span,
    pub level: u16,
}
pub(crate) struct Definition {
    pub prefix: Span,
    pub delimiters: Vec<Span>,
    pub body: Span,
    pub signature: Span,
    pub long: bool,
    pub uses_parameters: bool,
    pub failures: Vec<Vec<usize>>,
}
#[derive(Default)]
pub(crate) struct Packet {
    pub arguments: Vec<Sequence>,
    pub owners: Vec<(u32, Rc<Vec<Token>>)>,
}
impl Packet {
    pub fn clear(&mut self) {
        for argument in &mut self.arguments {
            argument.clear();
        }
        self.owners.clear();
    }
}
pub(crate) struct Capture {
    pub tokens: Rc<Vec<Token>>,
    pub building: bool,
}
// Cells use interior mutability solely for their one initialization. Published
// cells never change; chunk addresses stay stable and Engine retains all chunks.
// A cached cursor therefore remains valid while a disjoint suffix is initialized.
#[derive(Default)]
pub(crate) struct TokenArena {
    chunks: Vec<Box<[UnsafeCell<Token>]>>,
    pub len: u32,
}
impl TokenArena {
    pub fn push(&mut self, token: Token) -> Result<()> {
        if self.len == u32::MAX {
            return Err(Error::Capacity);
        }
        if (self.len as usize).is_multiple_of(CHUNK_LEN) {
            self.chunks
                .push((0..CHUNK_LEN).map(|_| UnsafeCell::new(Token(0))).collect());
        }
        let chunk = self.len as usize >> CHUNK_BITS;
        let offset = self.len as usize & (CHUNK_LEN - 1);
        // SAFETY: the cell is allocated and not yet published.
        unsafe {
            self.chunks[chunk][offset].get().write(token);
        }
        self.len += 1;
        Ok(())
    }
    pub fn get(&self, index: u32) -> Token {
        assert!(index < self.len);
        // SAFETY: the initialized prefix is immutable after publication.
        unsafe {
            *self.chunks[index as usize >> CHUNK_BITS][index as usize & (CHUNK_LEN - 1)].get()
        }
    }
    pub fn window(&self, start: u32, end: u32) -> (*const Token, *const Token, u32) {
        assert!(start < end && end <= self.len);
        let chunk = start as usize >> CHUNK_BITS;
        let offset = start as usize & (CHUNK_LEN - 1);
        let count = ((end - start) as usize).min(CHUNK_LEN - offset);
        let first = self.chunks[chunk][offset].get().cast_const();
        // SAFETY: UnsafeCell has Token's layout; count stays within this chunk.
        (first, unsafe { first.add(count) }, start + count as u32)
    }
}
pub(crate) struct Names {
    pub spellings: Vec<Box<[u8]>>,
    buckets: Vec<u32>,
    hashes: Vec<u64>,
}
impl Names {
    pub fn new() -> Self {
        let mut spellings: Vec<Box<[u8]>> = vec![b"relax".to_vec().into(), Box::default()];
        for _ in 0..2 {
            for b in 0..=255 {
                spellings.push(vec![b].into());
            }
        }
        Self {
            spellings,
            buckets: vec![0; 1024],
            hashes: vec![0; 514],
        }
    }
    pub fn intern(&mut self, name: &[u8]) -> Result<u32> {
        if name.is_empty() {
            return Ok(1);
        }
        if name.len() == 1 {
            return Ok(2 + u32::from(name[0]));
        }
        let mut hash = 0xcbf29ce484222325u64;
        for &b in name {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x100000001b3);
        }
        let mut slot = hash as usize & (self.buckets.len() - 1);
        loop {
            let id = self.buckets[slot];
            if id == 0 {
                break;
            }
            if self.hashes[id as usize] == hash && self.spellings[id as usize].as_ref() == name {
                return Ok(id);
            }
            slot = (slot + 1) & (self.buckets.len() - 1);
        }
        if self.spellings.len() >= CS as usize {
            return Err(Error::Capacity);
        }
        let id = self.spellings.len() as u32;
        self.spellings.push(name.into());
        self.hashes.push(hash);
        self.buckets[slot] = id;
        if (self.spellings.len() - 514) * 2 > self.buckets.len() {
            let mut buckets = vec![0; self.buckets.len() * 2];
            for id in 514..self.spellings.len() {
                let mut slot = self.hashes[id] as usize & (buckets.len() - 1);
                while buckets[slot] != 0 {
                    slot = (slot + 1) & (buckets.len() - 1);
                }
                buckets[slot] = id as u32;
            }
            self.buckets = buckets;
        }
        Ok(id)
    }
}
#[derive(Clone, Copy)]
pub(crate) enum SequenceRef {
    List,
    Body(u32),
    Argument(u32, u8),
}
pub(crate) struct Cursor {
    pub sequence: SequenceRef,
    pub span_index: usize,
    pub span: Span,
    pub window_end_position: u32,
    pub next: *const Token,
    pub end: *const Token,
    pub owns_packet: Option<u32>,
    pub macro_packet: Option<u32>,
}
pub(crate) enum Source {
    Tokens(Cursor),
    Inline {
        tokens: [Token; 2],
        position: u8,
        len: u8,
        suppress: bool,
    },
}
#[derive(Clone, Copy)]
pub(crate) struct Raw {
    pub token: Token,
    pub origin: Option<Span>,
    pub suppress: bool,
}
#[derive(Clone, Copy)]
pub(crate) struct Read {
    pub token: Token,
    pub meaning: Meaning,
}
