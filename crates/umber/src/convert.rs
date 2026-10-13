use crate::{Engine, Error, Result, scalars::Target, token::*};
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct Generated {
    first: Option<Token>,
    buffer: Option<u32>,
}

impl Engine<'_> {
    pub(crate) fn decimal_digits(
        &mut self,
        value: u32,
        emit: &mut impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        // At most ten ordinary calls emit directly in reading order. Decimal
        // characters cannot alter brace depth or parameter interpretation.
        if value >= 10 {
            self.decimal_digits(value / 10, emit)?;
        }
        emit(self, b'0' + (value % 10) as u8)
    }

    #[inline]
    pub(crate) fn generated_token(&mut self, output: &mut Generated, token: Token) -> Result<()> {
        if let Some(id) = output.buffer {
            let tokens = Rc::get_mut(&mut self.captures[id as usize].tokens).unwrap();
            if tokens.len() >= u32::MAX as usize {
                return Err(Error::Capacity);
            }
            tokens.push(token);
        } else if let Some(first) = output.first.take() {
            let id = self.capture_buffer()?;
            Rc::get_mut(&mut self.captures[id as usize].tokens)
                .unwrap()
                .extend_from_slice(&[first, token]);
            output.buffer = Some(id);
        } else {
            output.first = Some(token);
        }
        Ok(())
    }
    pub(crate) fn finish_generated(&mut self, output: Generated) -> Result<()> {
        if let Some(id) = output.buffer {
            self.push_generated(id)
        } else if let Some(token) = output.first {
            if self.sources.len() > 10000 {
                return Err(Error::Capacity);
            }
            self.back(token);
            Ok(())
        } else {
            Ok(())
        }
    }
    #[inline]
    fn generated_byte(&mut self, output: &mut Generated, byte: u8) -> Result<()> {
        let token = if byte == b' ' {
            Token::character(10, byte)
        } else {
            Token::other(byte)
        };
        self.generated_token(output, token)
    }
    pub(crate) fn token_text(
        &mut self,
        token: Token,
        show: bool,
        mut emit: impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        if !token.is_cs() {
            emit(self, token.byte())?;
            if show && token.cat() == 6 {
                emit(self, token.byte())?;
            }
            return Ok(());
        }
        let id = token.id();
        if (258..514).contains(&id) {
            return emit(self, (id - 258) as u8);
        }
        let escape = self.scalars[Target::ESCAPECHAR].value;
        if id == 1 {
            for word in [b"csname".as_slice(), b"endcsname"] {
                if (0..=255).contains(&escape) {
                    emit(self, escape as u8)?;
                }
                for &byte in word {
                    emit(self, byte)?;
                }
            }
            if show {
                emit(self, b' ')?;
            }
            return Ok(());
        }
        if (0..=255).contains(&escape) {
            emit(self, escape as u8)?;
        }
        let length = self.names.spelling(id).len();
        for index in 0..length {
            let byte = self.names.spelling(id)[index];
            emit(self, byte)?;
        }
        if show
            && (length != 1
                || self.scalars[Target::catcode(self.names.spelling(id)[0] as usize)].value == 11)
        {
            emit(self, b' ')?;
        }
        Ok(())
    }
    fn emit_text(
        &mut self,
        text: &[u8],
        emit: &mut impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        for &byte in text {
            emit(self, byte)?;
        }
        Ok(())
    }
    fn escaped_text(
        &mut self,
        text: &[u8],
        emit: &mut impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        let escape = self.scalars[Target::ESCAPECHAR].value;
        if (0..=255).contains(&escape) {
            emit(self, escape as u8)?;
        }
        self.emit_text(text, emit)
    }
    fn meaning_text(
        &mut self,
        read: Read,
        emit: &mut impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        let meaning = read.meaning;
        if meaning.is_macro() {
            let id = meaning.value as usize;
            let owner = self.values.get(id as u32);
            let definition = owner.definition();
            let long = definition.long;
            let protected = meaning.command == cmd::PROTECTED_MACRO;
            if protected {
                self.escaped_text(b"protected", emit)?;
            }
            if long {
                self.escaped_text(b"long", emit)?;
            }
            if protected || long {
                emit(self, b' ')?;
            }
            self.emit_text(b"macro:", emit)?;
            let signature = definition.signature();
            let body_start = definition.body.start;
            let mut marker = b'#';
            let mut parameter = 0;
            for position in signature.start..signature.end {
                let token = owner.tokens.get(position);
                if token.0 & PARAM != 0 && !token.is_cs() {
                    if position < body_start {
                        if token.byte() == 255 {
                            self.emit_text(b"->", emit)?;
                        } else {
                            marker = token.byte();
                            parameter += 1;
                            emit(self, marker)?;
                            emit(self, b'0' + parameter)?;
                        }
                    } else {
                        emit(self, marker)?;
                        emit(self, b'0' + token.byte())?;
                    }
                } else {
                    self.token_text(token, true, &mut *emit)?;
                }
            }
            return Ok(());
        }
        let prefix = match meaning.command {
            cmd::BEGIN => Some(b"begin-group character ".as_slice()),
            cmd::END_GROUP_CHAR => Some(b"end-group character ".as_slice()),
            3 => Some(b"math shift character ".as_slice()),
            4 => Some(b"alignment tab character ".as_slice()),
            6 => Some(b"macro parameter character ".as_slice()),
            7 => Some(b"superscript character ".as_slice()),
            8 => Some(b"subscript character ".as_slice()),
            cmd::SPACE => Some(b"blank space ".as_slice()),
            11 => Some(b"the letter ".as_slice()),
            12 => Some(b"the character ".as_slice()),
            _ => None,
        };
        if let Some(prefix) = prefix {
            self.emit_text(prefix, emit)?;
            return emit(self, meaning.value as u8);
        }
        if meaning.command == cmd::UNDEFINED {
            return self.emit_text(b"undefined", emit);
        }
        if meaning.command == cmd::CHAR_GIVEN {
            self.escaped_text(b"char\"", emit)?;
            let value = meaning.value as u8;
            if value >= 16 {
                emit(self, b"0123456789ABCDEF"[(value >> 4) as usize])?;
            }
            return emit(self, b"0123456789ABCDEF"[(value & 15) as usize]);
        }
        if matches!(meaning.command, cmd::COUNT_REF | cmd::TOKS_REF) {
            self.escaped_text(
                if meaning.command == cmd::COUNT_REF {
                    b"count"
                } else {
                    b"toks"
                },
                emit,
            )?;
            return self.decimal_digits(meaning.value as u32, emit);
        }
        let names = cmd::names(meaning.command);
        let index = if names.len() == 1 {
            0
        } else {
            meaning.value as usize
        };
        let name = names.get(index).ok_or(Error::Unsupported)?;
        self.escaped_text(name, emit)
    }
    pub(crate) fn convert(&mut self, read: Read) -> Result<()> {
        let mut output = Generated::default();
        self.conversion::<false>(read, |engine, byte| {
            engine.generated_byte(&mut output, byte)
        })?;
        self.finish_generated(output)
    }
    // Operands are read once; consumers choose their destination. The callback
    // appends only and preserves installed sources and immutable input spans.
    #[inline(never)]
    pub(crate) fn conversion<const DEFER_EMISSION: bool>(
        &mut self,
        read: Read,
        mut emit: impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        match read.meaning.command {
            cmd::STRING => {
                let token = self.next_raw()?.token;
                self.token_text(token, false, &mut emit)?;
            }
            cmd::MEANING => {
                let token = self.next_token()?;
                self.meaning_text(token, &mut emit)?;
            }
            cmd::DETOKENIZE => {
                self.opening_brace()?;
                let mut emission = Ok(());
                self.scan_raw_body(|engine, token| {
                    engine.token_text(token, true, |engine, byte| {
                        if DEFER_EMISSION {
                            // A direct destination error follows operand scan
                            // errors, as when the conversion used a token source.
                            if emission.is_ok() {
                                emission = emit(engine, byte);
                            }
                            Ok(())
                        } else {
                            emit(engine, byte)
                        }
                    })
                })?;
                emission?;
            }
            cmd::ROMAN => {
                let mut value = self.scan_int()?;
                for (number, text) in [
                    (1000, b"m".as_slice()),
                    (900, b"cm"),
                    (500, b"d"),
                    (400, b"cd"),
                    (100, b"c"),
                    (90, b"xc"),
                    (50, b"l"),
                    (40, b"xl"),
                    (10, b"x"),
                    (9, b"ix"),
                    (5, b"v"),
                    (4, b"iv"),
                    (1, b"i"),
                ] {
                    while value >= number {
                        self.emit_text(text, &mut emit)?;
                        value -= number;
                    }
                }
            }
            _ => return Err(Error::Unsupported),
        }
        Ok(())
    }
    pub(crate) fn change_case(&mut self, which: usize) -> Result<()> {
        self.opening_brace()?;
        let mut output = Generated::default();
        self.scan_raw_body(|engine, mut token| {
            let active = token.is_cs() && (258..514).contains(&token.id());
            if !token.is_cs() || active {
                let byte = if active {
                    (token.id() - 258) as u8
                } else {
                    token.byte()
                };
                let code = engine.scalars[Target::casecode(which, byte as usize)].value;
                if code != 0 {
                    token = if active {
                        Token::cs(258 + code as u32)
                    } else {
                        Token::character(token.cat(), code as u8)
                    };
                }
            }
            engine.generated_token(&mut output, token)
        })?;
        self.finish_generated(output)
    }
}
