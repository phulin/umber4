use crate::{Error, Result, names::Names, token::*, values::Builder};

// Lookup-only reads expose only a Meaning. A missing file name uses the first
// impossible name ID: Names::insert rejects IDs >= CS in release builds too.
// The binding table's out-of-range default resolves it as undefined. This token
// never reaches ordinary readers, stored input, assignments, or diagnostics.
const MISSING_NAME: Token = Token(2 * CS);

#[derive(Clone)]
pub(crate) struct FileInput<'a> {
    bytes: &'a [u8],
    pub name: &'a [u8],
    offset: usize,
    pub line: crate::value_metadata::Buffer<u8>,
    position: usize,
    state: u8,
    loaded: bool,
}
impl<'a> FileInput<'a> {
    #[cfg(feature = "lexing-prototype")]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self::new_in(
            crate::InputFile { name: b"", bytes },
            &std::rc::Rc::default(),
        )
    }
    pub fn new_in(
        file: crate::InputFile<'a>,
        store: &std::rc::Rc<crate::value_metadata::Store<u8>>,
    ) -> Self {
        Self {
            bytes: file.bytes,
            name: file.name,
            offset: 0,
            line: crate::value_metadata::Buffer::new(store),
            position: 0,
            state: 0,
            loaded: false,
        }
    }
    #[cfg_attr(not(feature = "native-profile"), inline(never))]
    pub(crate) fn load(&mut self, endline: i32) -> Result<()> {
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
        self.position = 0;
        self.state = 0;
        Ok(())
    }
    pub(crate) fn line_ended(&self) -> bool {
        self.position == self.line.len()
    }
    #[inline(always)]
    fn classified(&mut self, cats: &[Integer; 256]) -> (u8, u8) {
        let byte = self.line[self.position];
        let cat = cats[byte as usize].value as u8;
        if cat != 7 {
            return (byte, cat);
        }
        let byte = self.translated(cats);
        (byte, cats[byte as usize].value as u8)
    }
    #[inline(never)]
    fn translated(&mut self, cats: &[Integer; 256]) -> u8 {
        let p = self.position;
        loop {
            let b = self.line[p];
            if cats[b as usize].value != 7
                || self.line.get(p + 1) != Some(&b)
                || p + 2 >= self.line.len()
            {
                return b;
            }
            let hex = |c: u8| match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                _ => None,
            };
            let c = self.line[p + 2];
            let (decoded, end) =
                if let (Some(a), Some(b)) = (hex(c), self.line.get(p + 3).copied().and_then(hex)) {
                    (a * 16 + b, p + 4)
                } else if c < 128 {
                    (c ^ 64, p + 3)
                } else {
                    return b;
                };
            self.line[p] = decoded;
            let length = self.line.len() - (end - p - 1);
            self.line.copy_within(end.., p + 1);
            self.line.resize(length, 0);
        }
    }
    #[inline]
    pub fn read_character(&mut self, cats: &[Integer; 256]) -> Option<Token> {
        self.read_character_except::<false>(cats, Token(u32::MAX))
    }
    #[inline]
    pub fn literal_digit(&mut self, cats: &[Integer; 256]) -> Option<i32> {
        let &byte = self.line.get(self.position)?;
        let digit = byte.wrapping_sub(b'0');
        if digit >= 10 || cats[byte as usize].value != 12 {
            return None;
        }
        self.position += 1;
        self.state = 1;
        Some(i32::from(digit))
    }
    // Letters and other characters need no lexical state beyond their byte.
    #[inline(always)]
    fn character(cats: &[Integer; 256], byte: u8) -> Option<Token> {
        let cat = cats[byte as usize].value as u8;
        matches!(cat, 11 | 12).then(|| Token::character(cat, byte))
    }
    #[inline]
    pub fn read_character_except<const EXCEPT: bool>(
        &mut self,
        cats: &[Integer; 256],
        except: Token,
    ) -> Option<Token> {
        let token = Self::character(cats, *self.line.get(self.position)?)?;
        if EXCEPT && token == except {
            return None;
        }
        self.position += 1;
        self.state = 1;
        Some(token)
    }
    /// Tokenizes letters, other characters, and spaces directly into a
    /// builder, keeping the line position local for the whole run. A token
    /// equal to `except` ends the run unread.
    #[inline]
    pub fn read_characters<const EXCEPT: bool>(
        &mut self,
        cats: &[Integer; 256],
        except: Token,
        tokens: &mut Builder,
    ) -> Result<()> {
        let (line, mut position, mut state) = (&*self.line, self.position, self.state);
        let result = tokens.extend_with(|| {
            loop {
                let &byte = line.get(position)?;
                if let Some(token) = Self::character(cats, byte) {
                    if EXCEPT && token == except {
                        return None;
                    }
                    (position, state) = (position + 1, 1);
                    return Some(token);
                }
                if cats[byte as usize].value != 10 {
                    return None;
                }
                // As in read, only a space after a character is a token.
                if EXCEPT && state == 1 && Token::SPACE == except {
                    return None;
                }
                position += 1;
                if std::mem::replace(&mut state, 2) == 1 {
                    return Some(Token::SPACE);
                }
            }
        });
        self.position = position;
        self.state = state;
        result
    }
    #[cfg(feature = "lexing-prototype")]
    pub fn read(
        &mut self,
        cats: &[Integer; 256],
        endline: i32,
        names: &mut Names,
        par: Token,
    ) -> Result<Token> {
        self.read_with::<true>(cats, endline, names, par, false)
    }
    pub(super) fn read_with<const INTERN: bool>(
        &mut self,
        cats: &[Integer; 256],
        endline: i32,
        names: &mut Names,
        par: Token,
        force_eof: bool,
    ) -> Result<Token> {
        loop {
            if self.position == self.line.len() {
                if force_eof {
                    return Err(Error::Eof);
                }
                self.load(endline)?;
                continue;
            }
            let (b, cat) = self.classified(cats);
            self.position += 1;
            match cat {
                0 => {
                    if self.position == self.line.len() {
                        self.state = 2;
                        return Ok(Token::cs(NULL_CS));
                    }
                    let (first, cat) = self.classified(cats);
                    if cat == 11 {
                        let start = self.position;
                        let mut hash = Names::hash_byte(Names::HASH_START, first);
                        self.position += 1;
                        loop {
                            // Untranslated letters need no line rewrite, so
                            // the position stays local across the run.
                            let mut position = self.position;
                            while let Some(&byte) = self.line.get(position)
                                && cats[byte as usize].value as u8 == 11
                            {
                                hash = Names::hash_byte(hash, byte);
                                position += 1;
                            }
                            self.position = position;
                            if position == self.line.len() {
                                break;
                            }
                            // A superscript pair may still decode to a letter.
                            let (byte, cat) = self.classified(cats);
                            if cat != 11 {
                                break;
                            }
                            hash = Names::hash_byte(hash, byte);
                            self.position += 1;
                        }
                        self.state = 2;
                        return Ok(if self.position - start == 1 {
                            Token::single(first)
                        } else {
                            let name = &self.line[start..self.position];
                            let id = if INTERN {
                                names.intern_hashed(name, hash)?
                            } else if let Some(id) = names.lookup_hashed(name, hash) {
                                id
                            } else {
                                return Ok(MISSING_NAME);
                            };
                            Token::cs(id)
                        });
                    }
                    self.position += 1;
                    self.state = if cat == 10 { 2 } else { 1 };
                    return Ok(Token::single(first));
                }
                5 => {
                    self.position = self.line.len();
                    let old = self.state;
                    self.state = 0;
                    if old == 0 {
                        return Ok(par);
                    }
                    if old == 1 {
                        return Ok(Token::SPACE);
                    }
                }
                9 => {}
                10 => {
                    let old = self.state;
                    self.state = 2;
                    if old == 1 {
                        return Ok(Token::SPACE);
                    }
                }
                13 => {
                    self.state = 1;
                    return Ok(Token::active(b));
                }
                14 => {
                    self.position = self.line.len();
                }
                15 => return Err(Error::Syntax),
                _ => {
                    self.state = 1;
                    return Ok(Token::character(cat, b));
                }
            }
        }
    }
}
