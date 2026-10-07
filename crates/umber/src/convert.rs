use crate::{Engine, Error, Result, scalars::Target, token::*};
use std::rc::Rc;

impl Engine<'_> {
    #[inline]
    pub(crate) fn generated_token(
        &mut self,
        packet: u32,
        builder: &mut Option<u32>,
        token: Token,
    ) -> Result<()> {
        if let Some(id) = *builder {
            let tokens = Rc::get_mut(&mut self.captures[id as usize].tokens).unwrap();
            if tokens.len() >= u32::MAX as usize {
                return Err(Error::Capacity);
            }
            tokens.push(token);
        } else {
            let first = self.packets[packet as usize].arguments[0].first;
            if first.empty() {
                self.packets[packet as usize].arguments[0].first = Span {
                    store: SINGLE,
                    start: token.0,
                    end: token.0 + 1,
                };
            } else {
                let id = self.capture_buffer()?;
                Rc::get_mut(&mut self.captures[id as usize].tokens)
                    .unwrap()
                    .extend_from_slice(&[Token(first.start), token]);
                *builder = Some(id);
            }
        }
        Ok(())
    }
    pub(crate) fn finish_generated(&mut self, packet: u32, builder: Option<u32>) {
        if let Some(id) = builder {
            // The packet has no installed source while being built. Its dense
            // buffer has one writer; publish one range and owner at completion.
            self.packets[packet as usize].arguments[0].first = Span {
                store: id,
                start: 0,
                end: self.captures[id as usize].tokens.len() as u32,
            };
        }
        self.finish_capture(packet, builder);
    }
    #[inline]
    fn generated_byte(&mut self, packet: u32, builder: &mut Option<u32>, byte: u8) -> Result<()> {
        let token = if byte == b' ' {
            Token::character(10, byte)
        } else {
            Token::other(byte)
        };
        self.generated_token(packet, builder, token)
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
    fn conversion_text(
        &mut self,
        packet: u32,
        builder: &mut Option<u32>,
        text: &[u8],
    ) -> Result<()> {
        for &byte in text {
            self.generated_byte(packet, builder, byte)?;
        }
        Ok(())
    }
    fn escaped_text(&mut self, packet: u32, builder: &mut Option<u32>, text: &[u8]) -> Result<()> {
        let escape = self.scalars[Target::ESCAPECHAR].value;
        if (0..=255).contains(&escape) {
            self.generated_byte(packet, builder, escape as u8)?;
        }
        self.conversion_text(packet, builder, text)
    }
    fn meaning_tokens(&mut self, read: Read, packet: u32, builder: &mut Option<u32>) -> Result<()> {
        let meaning = read.meaning;
        if meaning.is_macro() {
            let id = meaning.value as usize;
            let long = self.definitions[id].long;
            let protected = meaning.command == cmd::PROTECTED_MACRO;
            if protected {
                self.escaped_text(packet, builder, b"protected")?;
            }
            if long {
                self.escaped_text(packet, builder, b"long")?;
            }
            if protected || long {
                self.generated_byte(packet, builder, b' ')?;
            }
            self.conversion_text(packet, builder, b"macro:")?;
            let signature = self.definitions[id].signature;
            let body_start = self.definitions[id].body.start;
            let mut marker = b'#';
            let mut parameter = 0;
            for position in signature.start..signature.end {
                let token = self.arena.get(position);
                if token.0 & PARAM != 0 && !token.is_cs() {
                    if position < body_start {
                        if token.byte() == 255 {
                            self.conversion_text(packet, builder, b"->")?;
                        } else {
                            marker = token.byte();
                            parameter += 1;
                            self.generated_byte(packet, builder, marker)?;
                            self.generated_byte(packet, builder, b'0' + parameter)?;
                        }
                    } else {
                        self.generated_byte(packet, builder, marker)?;
                        self.generated_byte(packet, builder, b'0' + token.byte())?;
                    }
                } else {
                    self.token_text(token, true, |engine, byte| {
                        engine.generated_byte(packet, builder, byte)
                    })?;
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
            self.conversion_text(packet, builder, prefix)?;
            return self.generated_byte(packet, builder, meaning.value as u8);
        }
        if meaning.command == cmd::UNDEFINED {
            return self.conversion_text(packet, builder, b"undefined");
        }
        if meaning.command == cmd::CHAR_GIVEN {
            self.escaped_text(packet, builder, b"char\"")?;
            let value = meaning.value as u8;
            if value >= 16 {
                self.generated_byte(packet, builder, b"0123456789ABCDEF"[(value >> 4) as usize])?;
            }
            return self.generated_byte(
                packet,
                builder,
                b"0123456789ABCDEF"[(value & 15) as usize],
            );
        }
        if matches!(meaning.command, cmd::COUNT_REF | cmd::TOKS_REF) {
            self.escaped_text(
                packet,
                builder,
                if meaning.command == cmd::COUNT_REF {
                    b"count"
                } else {
                    b"toks"
                },
            )?;
            let value = meaning.value.to_string();
            return self.conversion_text(packet, builder, value.as_bytes());
        }
        let name: &[u8] = match meaning.command {
            cmd::RELAX => b"relax",
            cmd::DEF => b"def",
            cmd::GDEF => b"gdef",
            cmd::EDEF => b"edef",
            cmd::XDEF => b"xdef",
            cmd::LET => b"let",
            cmd::GLOBAL => b"global",
            cmd::LONG => b"long",
            cmd::PROTECTED => b"protected",
            cmd::BEGIN_GROUP => b"begingroup",
            cmd::END_GROUP => b"endgroup",
            cmd::COUNT => b"count",
            cmd::COUNT_DEF => b"countdef",
            cmd::CATCODE => b"catcode",
            cmd::ENDLINE => b"endlinechar",
            cmd::MESSAGE => b"message",
            cmd::NUMBER => b"number",
            cmd::THE => b"the",
            cmd::EXPANDAFTER => b"expandafter",
            cmd::NOEXPAND => b"noexpand",
            cmd::CSNAME => b"csname",
            cmd::ENDCSNAME => b"endcsname",
            cmd::IFNUM => b"ifnum",
            cmd::IFX => b"ifx",
            cmd::IFTRUE => b"iftrue",
            cmd::IFFALSE => b"iffalse",
            cmd::ELSE => b"else",
            cmd::FI => b"fi",
            cmd::END => b"end",
            cmd::PAR => b"par",
            cmd::TOKS => b"toks",
            cmd::TOKS_DEF => b"toksdef",
            cmd::CHAR_DEF => b"chardef",
            cmd::FUTURELET => b"futurelet",
            cmd::AFTER_ASSIGN => b"afterassignment",
            cmd::AFTER_GROUP => b"aftergroup",
            cmd::NUMEXPR => b"numexpr",
            cmd::STRING => b"string",
            cmd::ROMAN => b"romannumeral",
            cmd::MEANING => b"meaning",
            cmd::IF => b"if",
            cmd::IFCAT => b"ifcat",
            cmd::IFODD => b"ifodd",
            cmd::IFCASE => b"ifcase",
            cmd::OR => b"or",
            cmd::IFDEFINED => b"ifdefined",
            cmd::IFCSNAME => b"ifcsname",
            cmd::UNEXPANDED => b"unexpanded",
            cmd::DETOKENIZE => b"detokenize",
            cmd::UNLESS => b"unless",
            cmd::ARITH => [b"advance".as_slice(), b"multiply", b"divide"][meaning.value as usize],
            cmd::CASE_CODE => {
                if meaning.value == 0 {
                    b"lccode"
                } else {
                    b"uccode"
                }
            }
            cmd::CHANGE_CASE => {
                if meaning.value == 0 {
                    b"lowercase"
                } else {
                    b"uppercase"
                }
            }
            cmd::INTEGER_PARAM => [
                b"globaldefs".as_slice(),
                b"escapechar",
                b"time",
                b"day",
                b"month",
                b"year",
            ][meaning.value as usize],
            _ => return Err(Error::Unsupported),
        };
        self.escaped_text(packet, builder, name)
    }
    pub(crate) fn convert(&mut self, read: Read) -> Result<()> {
        let packet = self.take_packet(1)?;
        let mut builder = None;
        match read.meaning.command {
            cmd::STRING => {
                let token = self.next_raw()?.token;
                self.token_text(token, false, |engine, byte| {
                    engine.generated_byte(packet, &mut builder, byte)
                })?;
            }
            cmd::MEANING => {
                let token = self.next_token()?;
                self.meaning_tokens(token, packet, &mut builder)?;
            }
            cmd::DETOKENIZE => {
                self.opening_brace()?;
                self.scan_raw_body(|engine, token| {
                    engine.token_text(token, true, |engine, byte| {
                        engine.generated_byte(packet, &mut builder, byte)
                    })
                })?;
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
                        self.conversion_text(packet, &mut builder, text)?;
                        value -= number;
                    }
                }
            }
            _ => return Err(Error::Unsupported),
        }
        self.finish_generated(packet, builder);
        self.push_sequence(SequenceRef::Argument(packet, 0), Some(packet), None)
    }
    pub(crate) fn change_case(&mut self, which: usize) -> Result<()> {
        self.opening_brace()?;
        let packet = self.take_packet(1)?;
        let mut builder = None;
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
            engine.generated_token(packet, &mut builder, token)
        })?;
        self.finish_generated(packet, builder);
        self.push_sequence(SequenceRef::Argument(packet, 0), Some(packet), None)
    }
}
