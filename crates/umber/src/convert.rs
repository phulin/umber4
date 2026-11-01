use crate::{Engine, Error, Result, scalars::Target, token::*};

#[derive(Default)]
pub(crate) struct Generated {
    first: Option<Token>,
    buffer: Option<crate::values::Builder>,
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
        if let Some(tokens) = &mut output.buffer {
            tokens.push(token)?;
        } else if let Some(first) = output.first.take() {
            let mut tokens = self.values.builder(crate::values::Class::Transient)?;
            tokens.push(first)?;
            tokens.push(token)?;
            output.buffer = Some(tokens);
        } else {
            output.first = Some(token);
        }
        Ok(())
    }
    pub(crate) fn finish_generated(&mut self, output: Generated) -> Result<()> {
        if let Some(tokens) = output.buffer {
            self.push_tokens(tokens.finish(), 0)
        } else if let Some(token) = output.first {
            self.check_source_limit()?;
            self.push_inline([token, Token(0)], 1, false);
            Ok(())
        } else {
            Ok(())
        }
    }
    #[inline(always)]
    pub(crate) fn token_text(
        &mut self,
        token: Token,
        show: bool,
        mut emit: impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        if token.is_cs() {
            return self.cs_text(token, show, emit);
        }
        emit(self, token.byte())?;
        if show && token.cat() == 6 {
            emit(self, token.byte())?;
        }
        Ok(())
    }
    // Character text stays inline in conversion loops; names spill here.
    #[inline(never)]
    fn cs_text(
        &mut self,
        token: Token,
        show: bool,
        mut emit: impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        if let Some(font) = token.font_id() {
            self.emit_escape(&mut emit)?;
            let id = self.document.fonts.descriptors[font].identifier;
            if id == NULL_CS as usize {
                self.emit_text(b"FONT", &mut emit)?;
            } else if (ACTIVE_BASE as usize..ACTIVE_BASE as usize + 256).contains(&id) {
                self.emit_text(b"FONT", &mut emit)?;
                emit(self, (id - ACTIVE_BASE as usize) as u8)?;
            } else {
                let length = self.names.spelling(id).len();
                for index in 0..length {
                    emit(self, self.names.spelling(id)[index])?;
                }
            }
            if show {
                emit(self, b' ')?;
            }
            return Ok(());
        }
        if let Some(byte) = token.active_byte() {
            return emit(self, byte);
        }
        let id = token.id();
        if id == NULL_CS as usize {
            self.escaped_text(b"csname", &mut emit)?;
            self.escaped_text(b"endcsname", &mut emit)?;
            if show {
                emit(self, b' ')?;
            }
            return Ok(());
        }
        self.emit_escape(&mut emit)?;
        let length = self.names.spelling(id).len();
        for index in 0..length {
            let byte = self.names.spelling(id)[index];
            emit(self, byte)?;
        }
        if show
            && (length != 1
                || self.document.scalars[Target::catcode(self.names.spelling(id)[0] as usize)]
                    .value
                    == 11)
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
    fn emit_escape(&mut self, emit: &mut impl FnMut(&mut Self, u8) -> Result<()>) -> Result<()> {
        let escape = self.document.scalars[Target::ESCAPECHAR].value;
        if (0..=255).contains(&escape) {
            emit(self, escape as u8)?;
        }
        Ok(())
    }
    fn escaped_text(
        &mut self,
        text: &[u8],
        emit: &mut impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        self.emit_escape(emit)?;
        self.emit_text(text, emit)
    }
    fn meaning_text(
        &mut self,
        read: Read,
        emit: &mut impl FnMut(&mut Self, u8) -> Result<()>,
    ) -> Result<()> {
        let meaning = read.meaning;
        if meaning.command == cmd::END_TEMPLATE {
            return self.escaped_text(b"outer endtemplate", emit);
        }
        if meaning.command == cmd::ENDV {
            return self.emit_text(b"end of alignment template", emit);
        }
        if meaning.command == 4 && meaning.value == 256 {
            return self.escaped_text(b"span", emit);
        }
        if meaning.command == cmd::ALIGN_CR && (257..=258).contains(&meaning.value) {
            return self.escaped_text(if meaning.value == 257 { b"cr" } else { b"crcr" }, emit);
        }
        if meaning.command == cmd::END_WRITE {
            self.escaped_text(b"outer", emit)?;
            return self.emit_text(b" macro:->", emit);
        }
        if meaning.command == cmd::SELECT_FONT {
            self.emit_text(b"select font ", emit)?;
            let font = self.document.fonts.descriptors[meaning.value as usize];
            if let Some(scaled) = font.scaled {
                let scaled = self.font_catalog.scaled[scaled].descriptor();
                let resource = scaled.resource;
                let size = scaled.size;
                let length = self.font_catalog.resources[resource].name.len();
                for index in 0..length {
                    emit(self, self.font_catalog.resources[resource].name[index])?;
                }
                if size != self.font_catalog.resources[resource].design {
                    self.emit_text(b" at ", emit)?;
                    let mut text = [0; 128];
                    let length = crate::quantities::quantity_text(
                        crate::quantities::Quantity::Dimension(size),
                        &mut text,
                    );
                    self.emit_text(&text[..length], emit)?;
                }
                return Ok(());
            }
            return self.emit_text(b"nullfont", emit);
        }
        if meaning.is_macro() {
            let id = meaning.value as usize;
            let owner = self.values.get(id as u32);
            let definition = owner.definition();
            let long = definition.long;
            let protected = meaning.is_protected_macro();
            let outer = meaning.is_outer_macro();
            if protected {
                self.escaped_text(b"protected", emit)?;
            }
            if long {
                self.escaped_text(b"long", emit)?;
            }
            if outer {
                self.escaped_text(b"outer", emit)?;
            }
            if protected || long || outer {
                emit(self, b' ')?;
            }
            self.emit_text(b"macro:", emit)?;
            let body_start = definition.body.start as usize;
            let mut marker = b'#';
            let mut parameter = 0;
            for (position, token) in owner.tokens.iter().enumerate() {
                if token.is_parameter() {
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
        if matches!(meaning.command, cmd::CHAR_GIVEN | cmd::MATH_GIVEN) {
            self.escaped_text(
                if meaning.command == cmd::MATH_GIVEN {
                    b"mathchar\""
                } else {
                    b"char\""
                },
                emit,
            )?;
            let value = meaning.value as u16;
            let mut leading = true;
            for shift in [12, 8, 4, 0] {
                let digit = (value >> shift) & 15;
                if digit != 0 || shift == 0 || !leading {
                    emit(self, b"0123456789ABCDEF"[digit as usize])?;
                    leading = false;
                }
            }
            return Ok(());
        }
        if matches!(
            meaning.command,
            cmd::COUNT_REF | cmd::TOKS_REF | cmd::DIMEN_REF | cmd::SKIP_REF | cmd::MUSKIP_REF
        ) {
            self.escaped_text(
                match meaning.command {
                    cmd::COUNT_REF => b"count".as_slice(),
                    cmd::TOKS_REF => b"toks".as_slice(),
                    cmd::DIMEN_REF => b"dimen".as_slice(),
                    cmd::MUSKIP_REF => b"muskip".as_slice(),
                    _ => b"skip".as_slice(),
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
            engine.generated_token(&mut output, Token::string_character(byte))
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
                let token = self
                    .with_scanner(crate::scanner::Scanner::Normal, |engine| engine.next_raw())?
                    .token;
                self.token_text(token, false, &mut emit)?;
            }
            cmd::MEANING => {
                let token = self.with_scanner(crate::scanner::Scanner::Normal, |engine| {
                    engine.next_token()
                })?;
                self.meaning_text(token, &mut emit)?;
            }
            cmd::DETOKENIZE => {
                let mut emission = Ok(());
                self.scan_raw_text(&mut |engine: &mut Self, token: Token| {
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
        let mut output = Generated::default();
        self.scan_raw_text(&mut |engine: &mut Self, mut token: Token| {
            let byte = if token.is_cs() {
                token.active_byte()
            } else {
                Some(token.byte())
            };
            if let Some(byte) = byte {
                let code = engine.document.scalars[Target::casecode(which, byte as usize)].value;
                if code != 0 {
                    // Case codes are validated bytes.
                    token = if token.is_cs() {
                        Token::active(code as u8)
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
