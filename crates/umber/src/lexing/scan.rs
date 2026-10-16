use super::hash::NameHash;
use super::{Counts, INVALID, Session, Source, Table, line};
use crate::names::Names;
use crate::token::{NULL_CS, Token};

/// Tokens a vector run write may store past the run's end.
pub(super) const SLACK: usize = 8;

/// Finds and writes character runs, and measures control-word letter runs.
/// A run byte is one whose token stands alone and leaves state M: a
/// character of category 1-4, 6, 8, 11, or 12, or an active character. A run
/// starts at one and continues through them and single blank spaces between
/// them, so each byte is one token: such a space follows state M and precedes
/// another run byte.
pub(super) trait Scan {
    /// Writes the run starting at `p` and returns its length, at most `limit`.
    fn run(&self, text: &[u8], p: usize, limit: usize, out: &mut Out) -> usize;
    /// Counts letters starting at `p`, at most `limit`.
    fn letters(&self, text: &[u8], p: usize, limit: usize) -> usize;
    /// Counts category-9 and category-10 bytes starting at `p`, at most `limit`.
    fn spaces(&self, text: &[u8], p: usize, limit: usize) -> usize;
}

/// Writes tokens into capacity reserved for the whole file. Every token
/// consumes at least one source byte, except the one end-of-line character
/// per line, and every line but the last consumes a terminator byte that
/// yields no token. A file of `n` bytes therefore yields at most `n + 1`.
pub(super) struct Out<'a> {
    tokens: &'a mut Vec<u32>,
    len: usize,
    #[cfg(debug_assertions)]
    limit: usize,
}
impl<'a> Out<'a> {
    fn new(tokens: &'a mut Vec<u32>, bound: usize) -> Self {
        tokens.reserve(bound + SLACK);
        Self {
            len: tokens.len(),
            #[cfg(debug_assertions)]
            limit: tokens.len() + bound,
            tokens,
        }
    }
    #[inline(always)]
    pub fn push(&mut self, token: u32) {
        #[cfg(debug_assertions)]
        assert!(self.len < self.limit);
        // SAFETY: new reserved the file's token bound, which len stays below.
        unsafe { self.tokens.as_mut_ptr().add(self.len).write(token) };
        self.len += 1;
    }
    /// The next token cell. `SLACK` cells past the file's bound are writable.
    #[cfg(target_arch = "x86_64")]
    #[inline(always)]
    pub fn tail(&mut self) -> *mut u32 {
        // SAFETY: len is within the reserved allocation.
        unsafe { self.tokens.as_mut_ptr().add(self.len) }
    }
    /// Publishes `n` cells written through `tail`.
    #[cfg(target_arch = "x86_64")]
    #[inline(always)]
    pub fn advance(&mut self, n: usize) {
        #[cfg(debug_assertions)]
        assert!(self.len + n <= self.limit);
        self.len += n;
    }
    fn finish(self) {
        // SAFETY: every cell below len was written as an initialized u32.
        unsafe { self.tokens.set_len(self.len) };
    }
}

/// The token of each run byte, and zero for the others.
pub(super) fn run_tokens(cats: &[u8; 256]) -> [u32; 256] {
    std::array::from_fn(|byte| {
        let (byte, cat) = (byte as u8, cats[byte]);
        match cat {
            1..=4 | 6 | 8 | 11 | 12 => Token::character(cat, byte).0,
            13 => Token::active(byte).0,
            _ => 0,
        }
    })
}

/// Byte loops over the run token table and categories.
pub(super) struct Bytes {
    tokens: [u32; 256],
    cats: [u8; 256],
}
impl Bytes {
    pub fn new(table: &Table) -> Self {
        Self {
            tokens: run_tokens(&table.cats),
            cats: table.cats,
        }
    }
    #[inline(always)]
    fn count(&self, text: &[u8], p: usize, limit: usize, member: impl Fn(u8) -> bool) -> usize {
        let mut n = 0;
        while n < limit && member(self.cats[text[p + n] as usize]) {
            n += 1;
        }
        n
    }
}
impl Scan for Bytes {
    #[inline(always)]
    fn run(&self, text: &[u8], p: usize, limit: usize, out: &mut Out) -> usize {
        let mut n = 0;
        while n < limit {
            let byte = text[p + n];
            let token = self.tokens[byte as usize];
            if token != 0 {
                out.push(token);
            } else if byte == b' '
                && self.cats[32] == 10
                && n + 1 < limit
                && self.tokens[text[p + n + 1] as usize] != 0
            {
                out.push(Token::SPACE.0);
            } else {
                break;
            }
            n += 1;
        }
        n
    }
    #[inline(always)]
    fn letters(&self, text: &[u8], p: usize, limit: usize) -> usize {
        self.count(text, p, limit, |cat| cat == 11)
    }
    #[inline(always)]
    fn spaces(&self, text: &[u8], p: usize, limit: usize) -> usize {
        self.count(text, p, limit, |cat| cat == 9 || cat == 10)
    }
}

/// Lexes the whole source. Lines split and lose trailing spaces as in the
/// current loader, but are read in place; the end-of-line character is
/// handled where the line's content ends.
#[inline(always)]
pub(super) fn lex<S: Scan, H: NameHash>(
    scan: &S,
    source: &Source,
    table: &Table,
    session: &mut Session,
    tokens: &mut Vec<u32>,
) -> Counts {
    let text = &source.bytes[..];
    let len = source.len();
    let mut out = Out::new(tokens, len + 1);
    let mut counts = Counts::default();
    let mut offset = 0;
    loop {
        let start = offset;
        let end = start + memchr::memchr2(b'\n', b'\r', &text[start..len]).unwrap_or(len - start);
        offset = end;
        if offset < len {
            let terminator = text[offset];
            offset += 1;
            if terminator == b'\r' && offset < len && text[offset] == b'\n' {
                offset += 1;
            }
        }
        let mut content = end;
        while content > start && text[content - 1] == b' ' {
            content -= 1;
        }
        counts.lines += 1;
        let line = Line {
            text,
            start,
            end: content,
            cats: &table.cats,
            endline: u8::try_from(table.endline).ok(),
        };
        if let Err((p, state)) = line.lex::<S, H>(scan, &mut session.names, session.par.0, &mut out)
        {
            counts.fallback_lines += 1;
            let prepared = &mut session.line;
            prepared.clear();
            prepared.extend_from_slice(&text[start..content]);
            prepared.extend(line.endline);
            let at = p - start;
            line::lex::<H>(
                prepared,
                at,
                state,
                &table.cats,
                &mut session.names,
                session.par.0,
                &mut out,
            );
        }
        if offset == len {
            break;
        }
    }
    out.finish();
    counts
}

struct Line<'a> {
    text: &'a [u8],
    start: usize,
    /// The end of the content before trailing spaces and the terminator.
    end: usize,
    cats: &'a [u8; 256],
    endline: Option<u8>,
}
impl Line<'_> {
    /// Whether the cat-7 byte at `p` begins a `^^` pair that the reader
    /// would translate, counting the end-of-line character as a final byte.
    #[inline(always)]
    fn translates(&self, p: usize) -> bool {
        p + 1 < self.end
            && self.text[p + 1] == self.text[p]
            && (p + 2 < self.end || self.endline.is_some())
    }

    /// Lexes the line, or returns the position and lexical state of the
    /// first token the fast path does not read: a `^^` pair the reader would
    /// translate, or a control word continuing into a letter end-of-line
    /// character.
    #[inline(always)]
    fn lex<S: Scan, H: NameHash>(
        &self,
        scan: &S,
        names: &mut Names,
        par: u32,
        out: &mut Out,
    ) -> Result<(), (usize, u8)> {
        let (text, cats, e) = (self.text, self.cats, self.end);
        let mut p = self.start;
        let mut state = 0;
        while p < e {
            let byte = text[p];
            let cat = cats[byte as usize];
            match cat {
                1..=4 | 6 | 8 | 11..=13 => {
                    p += scan.run(text, p, e - p, out);
                    state = 1;
                }
                10 => {
                    if std::mem::replace(&mut state, 2) == 1 {
                        out.push(Token::SPACE.0);
                    }
                    // Later spaces and ignored characters change nothing.
                    p += 1 + scan.spaces(text, p + 1, e - p - 1);
                }
                0 => {
                    let q = p + 1;
                    if q == e {
                        // The escape takes the end-of-line character, if any.
                        match self.endline {
                            None => out.push(Token::cs(NULL_CS).0),
                            Some(c) if cats[c as usize] == 11 => return Err((p, state)),
                            Some(c) => out.push(Token::single(c).0),
                        }
                        return Ok(());
                    }
                    let first = text[q];
                    let first_cat = cats[first as usize];
                    if first_cat == 11 {
                        let end = q + 1 + scan.letters(text, q + 1, e - q - 1);
                        let stops = if end == e {
                            self.endline.is_some_and(|c| cats[c as usize] == 11)
                        } else {
                            cats[text[end] as usize] == 7 && self.translates(end)
                        };
                        if stops {
                            return Err((p, state));
                        }
                        let n = end - q;
                        out.push(if n == 1 {
                            Token::single(first).0
                        } else {
                            let id = names.intern_hashed(&text[q..end], H::padded(text, q, n));
                            Token::cs(id.unwrap()).0
                        });
                        state = 2;
                        p = end;
                    } else if first_cat == 7 && self.translates(q) {
                        return Err((p, state));
                    } else {
                        out.push(Token::single(first).0);
                        state = if first_cat == 10 { 2 } else { 1 };
                        p = q + 1;
                    }
                }
                5 => {
                    end_line(state, par, out);
                    return Ok(());
                }
                9 => p += 1,
                14 => return Ok(()),
                15 => {
                    out.push(INVALID);
                    p += 1;
                }
                7 if self.translates(p) => return Err((p, state)),
                _ => {
                    out.push(Token::character(cat, byte).0);
                    state = 1;
                    p += 1;
                }
            }
        }
        let Some(c) = self.endline else {
            return Ok(());
        };
        match cats[c as usize] {
            5 => end_line(state, par, out),
            0 => out.push(Token::cs(NULL_CS).0),
            9 | 14 => {}
            10 => {
                if state == 1 {
                    out.push(Token::SPACE.0);
                }
            }
            13 => out.push(Token::active(c).0),
            15 => out.push(INVALID),
            cat => out.push(Token::character(cat, c).0),
        }
        Ok(())
    }
}

/// An end-of-line category character: `\par` on an empty line, a space after
/// a character, and nothing after a space or control word.
#[inline(always)]
fn end_line(state: u8, par: u32, out: &mut Out) {
    match state {
        0 => out.push(par),
        1 => out.push(Token::SPACE.0),
        _ => {}
    }
}
