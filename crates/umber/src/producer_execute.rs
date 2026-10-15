use super::*;
impl<const TRACE: bool> Producer<'_, TRACE> {
    #[inline(always)]
    pub(super) fn execute(&mut self, input: &mut Frame, mut read: Read) -> Result<()> {
        let mut prefix = 0;
        loop {
            match read.meaning.command {
                cmd::GLOBAL => prefix |= 1,
                cmd::LONG => prefix |= 2,
                cmd::RELAX | cmd::SPACE | cmd::PAR if prefix == 0 => return Ok(()),
                cmd::RELAX | cmd::SPACE | cmd::PAR => {}
                _ => break,
            }
            read = self.expanded(input, 0)?;
        }
        let command = read.meaning.command;
        let global = prefix & 1 != 0;
        if prefix & 2 != 0 && !matches!(command, cmd::DEF | cmd::GDEF) {
            return Err(Error::Syntax);
        }
        match command {
            cmd::COUNT | cmd::CATCODE | cmd::ENDLINE => {
                let target = if command == cmd::ENDLINE {
                    Target::ENDLINE
                } else {
                    let index = scan::index(self.integer(input, 0)?)?;
                    if command == cmd::COUNT {
                        Target::count(index)
                    } else {
                        Target::catcode(index)
                    }
                };
                self.equals(input)?;
                let value = self.integer(input, 0)?;
                if command == cmd::CATCODE && !(0..=15).contains(&value) {
                    return Err(Error::Number);
                }
                self.engine.assign_integer(target, global, value);
            }
            cmd::ARITH => {
                if self.nonspace::<true>(input, 0)?.meaning.command != cmd::COUNT {
                    return Err(Error::Unsupported);
                }
                let index = scan::index(self.integer(input, 0)?)?;
                self.by(input)?;
                let value = self.integer(input, 0)?;
                let target = Target::count(index);
                let old = self.engine.scalars[target].value;
                let value = match read.meaning.value {
                    0 => old.wrapping_add(value),
                    1 => {
                        let product = i64::from(old) * i64::from(value);
                        if product.abs() > i64::from(i32::MAX) {
                            return Err(Error::Overflow);
                        }
                        product as i32
                    }
                    2 => old.checked_div(value).ok_or(Error::Overflow)?,
                    _ => unreachable!(),
                };
                self.engine.assign_integer(target, global, value);
            }
            cmd::BEGIN | cmd::BEGIN_GROUP if !global => {
                self.engine.level = self.engine.level.checked_add(1).ok_or(Error::Capacity)?;
                self.engine
                    .save
                    .push(crate::execute::Save::Boundary(command == cmd::BEGIN_GROUP));
            }
            cmd::END_GROUP_CHAR | cmd::END_GROUP if !global => {
                self.engine.close_group(command == cmd::END_GROUP)?;
            }
            cmd::DEF | cmd::GDEF => self.define(input, global || command == cmd::GDEF)?,
            cmd::LET => self.alias(input, global)?,
            cmd::MESSAGE if !global => self.message(input)?,
            cmd::END if !global => {
                if self.engine.level != 0 {
                    return Err(Error::Group);
                }
                if !self.engine.conditions.is_empty() {
                    return Err(Error::Conditional);
                }
                self.ended = true;
            }
            _ => return Err(Error::Unsupported),
        }
        Ok(())
    }
    fn define(&mut self, input: &mut Frame, global: bool) -> Result<()> {
        let target = self.nonspace::<false>(input, 0)?.token;
        if !target.is_cs() {
            return Err(Error::Syntax);
        }
        let mut tokens = self.engine.values.builder()?;
        if !self.raw(input)?.token.is_begin() {
            return Err(Error::Unsupported);
        }
        tokens.push(Token(PARAM | 255))?;
        let mut depth = 1;
        while depth != 0 {
            let token = self.raw(input)?.token;
            if token.cat() == 6 && !token.is_cs() {
                return Err(Error::Unsupported);
            }
            if token.is_begin() {
                depth += 1;
            }
            if token.is_end() {
                depth -= 1;
            }
            if depth != 0 {
                tokens.push(token)?;
            }
        }
        let end = tokens.len();
        let owner = tokens.finish(Some(Definition {
            prefix_end: 0,
            body: ArenaRange { start: 1, end },
            parameters: Box::new([]),
            long: false,
            uses_parameters: false,
        }));
        self.engine.assign_meaning(
            target.id(),
            Meaning {
                command: cmd::MACRO,
                value: owner.id as i32,
            },
            global,
        );
        Ok(())
    }
    fn alias(&mut self, input: &mut Frame, global: bool) -> Result<()> {
        let target = self.nonspace::<false>(input, 0)?.token;
        if !target.is_cs() {
            return Err(Error::Syntax);
        }
        let mut right = self.nonspace::<false>(input, 0)?;
        if right.token == Token::other(b'=') {
            right = self.raw(input)?;
            if right.token.is_space() {
                right = self.raw(input)?;
            }
        }
        self.engine
            .assign_meaning(target.id(), right.meaning, global);
        Ok(())
    }
    fn message(&mut self, input: &mut Frame) -> Result<()> {
        let open = loop {
            let read = self.expanded(input, 0)?;
            if !matches!(read.meaning.command, cmd::SPACE | cmd::RELAX) {
                break read;
            }
        };
        if open.meaning.command != cmd::BEGIN {
            return Err(Error::Syntax);
        }
        let mut output = Vec::new();
        let mut depth = 1;
        while depth != 0 {
            let read = self.expanded(input, 0)?;
            if read.meaning.command == cmd::BEGIN {
                depth += 1;
            }
            if read.meaning.command == cmd::END_GROUP_CHAR {
                depth -= 1;
            }
            if depth != 0 {
                self.engine.print_token(read.token, &mut output);
            }
        }
        self.engine.messages.push(output);
        Ok(())
    }
}
