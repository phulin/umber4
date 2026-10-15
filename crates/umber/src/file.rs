use crate::{Error, Result, names::Names, token::*};
use std::mem::MaybeUninit;

// One lexer step. Lookahead returns Stop where the committed reader would
// translate a `^^` pair, insert a name, or report an invalid character after
// earlier tokens execute. Both return LineEnd where a line would load.
enum Lexed {
    Token(Token),
    LineEnd,
    Stop,
}

// A line position and TeX's lexical state: 0 at the line start, 1 mid-line,
// 2 skipping blanks.
#[derive(Clone, Copy)]
struct At {
    position: usize,
    state: u8,
}
const LINE_START: At = At {
    position: 0,
    state: 0,
};

// File input lexes the rest of its prepared line ahead into a reusable token
// window (docs/input.md section 3). Lookahead begins at start; a category write
// relexes the consumed tokens from there to find the first unconsumed one, so
// every token still takes the tables in force when it is read.
pub(crate) struct FileInput<'a> {
    bytes: &'a [u8],
    offset: usize,
    line: Vec<u8>,
    at: At,
    start: At,
    loaded: bool,
    tokens: Vec<Token>,
    // The unconsumed lookahead within tokens. Consumers only advance next;
    // refill and rewind replace the buffer and reset both pointers.
    pub next: *const Token,
    pub end: *const Token,
}
impl<'a> FileInput<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        let tokens = Vec::new();
        let next = tokens.as_ptr();
        Self {
            bytes,
            offset: 0,
            line: Vec::new(),
            at: LINE_START,
            start: LINE_START,
            loaded: false,
            tokens,
            next,
            end: next,
        }
    }
    #[cfg_attr(not(feature = "native-profile"), inline(never))]
    fn load(&mut self, endline: i32) -> Result<()> {
        if self.loaded && self.offset == self.bytes.len() {
            return Err(Error::Eof);
        }
        self.loaded = true;
        self.line.clear();
        let start = self.offset;
        let remaining = &self.bytes[start..];
        let mut offset =
            start + memchr::memchr2(b'\n', b'\r', remaining).unwrap_or(remaining.len());
        let mut end = offset;
        while end > start && self.bytes[end - 1] == b' ' {
            end -= 1;
        }
        self.line.extend_from_slice(&self.bytes[start..end]);
        if offset < self.bytes.len() {
            let b = self.bytes[offset];
            offset += 1;
            if b == b'\r' && self.bytes.get(offset) == Some(&b'\n') {
                offset += 1;
            }
        }
        self.offset = offset;
        if (0..=255).contains(&endline) {
            self.line.push(endline as u8);
        }
        self.at = LINE_START;
        Ok(())
    }
    /// Reads the next token once the lookahead window is exhausted, then lexes
    /// ahead behind it. A stop token is read on the committed path.
    #[inline(never)]
    pub fn refill(
        &mut self,
        cats: &[Integer; 256],
        endline: i32,
        names: &mut Names,
        par: Token,
    ) -> Result<Token> {
        self.discard();
        loop {
            // Only a token needed after the line's last one loads the next.
            if self.at.position == self.line.len() {
                self.load(endline)?;
            }
            self.speculate(cats, names, par);
            if let Some(&token) = self.tokens.first() {
                self.open(1);
                return Ok(token);
            }
            if self.at.position < self.line.len() {
                break;
            }
        }
        let token = self.read(cats, endline, names, par)?;
        self.speculate(cats, names, par);
        self.open(0);
        Ok(token)
    }
    // The committed reader: one token, loading lines as TeX's get_next does.
    fn read(
        &mut self,
        cats: &[Integer; 256],
        endline: i32,
        names: &mut Names,
        par: Token,
    ) -> Result<Token> {
        loop {
            let mut at = self.at;
            let lexed = lex::<true>(&mut self.line, &mut at, cats, names, par);
            self.at = at;
            match lexed? {
                Lexed::Token(token) => return Ok(token),
                _ => self.load(endline)?,
            }
        }
    }
    // Lexes ahead into the empty buffer, to the line end or a stop. The lexer
    // ends past any trailing blanks, or before a stop's skipped blanks.
    fn speculate(&mut self, cats: &[Integer; 256], names: &mut Names, par: Token) {
        self.start = self.at;
        // Lookahead never rewrites the line, and each token consumes at least
        // one of its remaining bytes.
        self.tokens.reserve(self.line.len() - self.at.position);
        let tokens = self.tokens.spare_capacity_mut();
        let mut at = self.at;
        let mut count = 0;
        loop {
            count = characters(&self.line, &mut at, cats, tokens, count);
            let mark = at;
            match lex::<false>(&mut self.line, &mut at, cats, names, par) {
                Ok(Lexed::Token(token)) => {
                    tokens[count].write(token);
                    count += 1;
                }
                Ok(Lexed::LineEnd) => break,
                _ => {
                    at = mark;
                    break;
                }
            }
        }
        self.at = at;
        // SAFETY: the loop initialized the first count spare tokens.
        unsafe { self.tokens.set_len(count) };
    }
    /// Discards unconsumed lookahead before a category write takes effect.
    /// Relexing the consumed tokens under the tables that produced them
    /// returns the lexer to the first unconsumed token, or past the last
    /// token, before any blanks or comment that followed it.
    pub fn rewind(&mut self, cats: &[Integer; 256], names: &mut Names, par: Token) {
        let consumed = (self.next.addr() - self.tokens.as_ptr().addr()) / size_of::<Token>();
        if !self.tokens.is_empty() {
            let mut at = self.start;
            for _ in 0..consumed {
                // Each relexed token was buffered from these same positions.
                let _ = lex::<false>(&mut self.line, &mut at, cats, names, par);
            }
            self.at = at;
        }
        self.discard();
    }
    fn discard(&mut self) {
        self.tokens.clear();
        self.start = self.at;
        self.next = self.tokens.as_ptr();
        self.end = self.next;
    }
    fn open(&mut self, start: usize) {
        self.next = self.tokens[start..].as_ptr();
        self.end = self.tokens.as_ptr_range().end;
    }
}

// Letters, other characters, and blanks dominate prepared lines. Lexes their
// run as lex would, without its general dispatch, and returns the new count.
#[inline(always)]
fn characters(
    line: &[u8],
    at: &mut At,
    cats: &[Integer; 256],
    tokens: &mut [MaybeUninit<Token>],
    mut count: usize,
) -> usize {
    let (mut position, mut state) = (at.position, at.state);
    let base = tokens.as_mut_ptr();
    while let Some(&byte) = line.get(position) {
        let cat = cats[byte as usize].value as u8;
        if matches!(cat, 11 | 12) {
            unsafe { base.add(count).write(MaybeUninit::new(Token::character(cat, byte))) };
            count += 1;
            position += 1;
            state = 1;
            continue;
        }
        if cat != 10 {
            break;
        }
        position += 1;
        if state == 1 {
            unsafe { base.add(count).write(MaybeUninit::new(Token::SPACE)) };
            count += 1;
        }
        state = 2;
    }
    *at = At { position, state };
    count
}

// Lookahead stops at any pair that might translate rather than rewrite the
// line ahead of execution.
#[inline(always)]
fn classified<const COMMIT: bool>(
    line: &mut Vec<u8>,
    position: usize,
    cats: &[Integer; 256],
) -> Option<(u8, u8)> {
    let byte = line[position];
    let cat = cats[byte as usize].value as u8;
    if cat != 7 {
        return Some((byte, cat));
    }
    if !COMMIT {
        let pair = line.get(position + 1) == Some(&byte) && position + 2 < line.len();
        return (!pair).then_some((byte, cat));
    }
    let byte = translated(line, position, cats);
    Some((byte, cats[byte as usize].value as u8))
}

#[inline(never)]
fn translated(line: &mut Vec<u8>, p: usize, cats: &[Integer; 256]) -> u8 {
    loop {
        let b = line[p];
        if cats[b as usize].value != 7 || line.get(p + 1) != Some(&b) || p + 2 >= line.len() {
            return b;
        }
        let hex = |c: u8| match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            _ => None,
        };
        let c = line[p + 2];
        let (decoded, end) =
            if let (Some(a), Some(b)) = (hex(c), line.get(p + 3).copied().and_then(hex)) {
                (a * 16 + b, p + 4)
            } else if c < 128 {
                (c ^ 64, p + 3)
            } else {
                return b;
            };
        line[p] = decoded;
        line.drain(p + 1..end);
    }
}

// TeX's get_next over the prepared line, from a position kept in registers.
// Lookahead never fails: it stops instead of taking any effect beyond that
// position and lexical state.
#[inline(always)]
fn lex<const COMMIT: bool>(
    line: &mut Vec<u8>,
    at: &mut At,
    cats: &[Integer; 256],
    names: &mut Names,
    par: Token,
) -> Result<Lexed> {
    loop {
        if at.position == line.len() {
            return Ok(Lexed::LineEnd);
        }
        let Some((b, cat)) = classified::<COMMIT>(line, at.position, cats) else {
            return Ok(Lexed::Stop);
        };
        at.position += 1;
        match cat {
            0 => {
                if at.position == line.len() {
                    at.state = 2;
                    return Ok(Lexed::Token(Token::cs(NULL_CS)));
                }
                let Some((first, cat)) = classified::<COMMIT>(line, at.position, cats) else {
                    return Ok(Lexed::Stop);
                };
                if cat == 11 {
                    let start = at.position;
                    let mut hash = Names::hash_byte(Names::HASH_START, first);
                    let mut position = start + 1;
                    loop {
                        // Untranslated letters need no line rewrite.
                        while let Some(&byte) = line.get(position)
                            && cats[byte as usize].value as u8 == 11
                        {
                            hash = Names::hash_byte(hash, byte);
                            position += 1;
                        }
                        if position == line.len() {
                            break;
                        }
                        // A superscript pair may still decode to a letter.
                        let Some((byte, cat)) = classified::<COMMIT>(line, position, cats) else {
                            return Ok(Lexed::Stop);
                        };
                        if cat != 11 {
                            break;
                        }
                        hash = Names::hash_byte(hash, byte);
                        position += 1;
                    }
                    at.position = position;
                    at.state = 2;
                    let name = &line[start..position];
                    let token = if name.len() == 1 {
                        Token::single(first)
                    } else if COMMIT {
                        Token::cs(names.intern_hashed(name, hash)?)
                    } else {
                        // New names enter the table in consumption order.
                        match names.lookup_hashed(name, hash) {
                            Some(id) => Token::cs(id),
                            None => return Ok(Lexed::Stop),
                        }
                    };
                    return Ok(Lexed::Token(token));
                }
                at.position += 1;
                at.state = if cat == 10 { 2 } else { 1 };
                return Ok(Lexed::Token(Token::single(first)));
            }
            5 => {
                at.position = line.len();
                let old = at.state;
                at.state = 0;
                if old == 0 {
                    return Ok(Lexed::Token(par));
                }
                if old == 1 {
                    return Ok(Lexed::Token(Token::SPACE));
                }
            }
            9 => {}
            10 => {
                let old = at.state;
                at.state = 2;
                if old == 1 {
                    return Ok(Lexed::Token(Token::SPACE));
                }
            }
            13 => {
                at.state = 1;
                return Ok(Lexed::Token(Token::active(b)));
            }
            14 => {
                at.position = line.len();
            }
            15 if COMMIT => return Err(Error::Syntax),
            15 => return Ok(Lexed::Stop),
            _ => {
                at.state = 1;
                return Ok(Lexed::Token(Token::character(cat, b)));
            }
        }
    }
}
