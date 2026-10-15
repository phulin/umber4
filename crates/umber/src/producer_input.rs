use super::*;
impl Body {
    #[inline(always)]
    pub(super) fn new(owner: values::Owner) -> Self {
        let range = owner.definition().body;
        let (next, end, window_end) = if range.start == range.end {
            (std::ptr::null(), std::ptr::null(), range.end)
        } else {
            owner.tokens.window(range.start, range.end)
        };
        Self {
            owner,
            next,
            end,
            window_end,
            limit: range.end,
        }
    }
    #[inline(always)]
    pub(super) fn restart(&mut self) {
        let start = self.owner.definition().body.start;
        if start != self.limit {
            (self.next, self.end, self.window_end) = self.owner.tokens.window(start, self.limit);
        }
    }
    #[inline(never)]
    fn refill(&mut self) -> bool {
        if self.window_end == self.limit {
            return false;
        }
        (self.next, self.end, self.window_end) =
            self.owner.tokens.window(self.window_end, self.limit);
        true
    }
    #[inline(always)]
    fn take_literal<const KEYWORD: bool>(&mut self, word: &[u8]) -> bool {
        // SAFETY: these cursors delimit one retained immutable window. The
        // length check covers every read; failure leaves the cursor unchanged.
        unsafe {
            if self.next == self.end || (self.end.offset_from(self.next) as usize) < word.len() {
                return false;
            }
            if !word.iter().enumerate().all(|(index, &byte)| {
                let token = self.next.add(index).read();
                if KEYWORD {
                    (index != 0 || token.cat() != 10)
                        && !token.is_cs()
                        && token.byte().eq_ignore_ascii_case(&byte)
                } else {
                    token == Token::other(byte)
                }
            }) {
                return false;
            }
            self.next = self.next.add(word.len());
        }
        true
    }
    #[inline(always)]
    fn next(&mut self) -> Option<Token> {
        if self.next == self.end && (self.window_end == self.limit || !self.refill()) {
            return None;
        }
        // SAFETY: the body owner retains the initialized immutable window;
        // refill covers only the published body, and the cursor stays in it.
        let token = unsafe { self.next.read() };
        self.next = unsafe { self.next.add(1) };
        Some(token)
    }
    #[inline(always)]
    fn digits(&mut self, value: &mut i32, digits: &mut bool) -> Result<Option<Token>> {
        let mut found = *digits;
        loop {
            let mut next = self.next;
            let mut accumulated = *value;
            while next != self.end {
                // SAFETY: owner retains this initialized immutable window.
                // The terminator is observed without consuming it; this loop
                // calls no expander or other input operation.
                let token = unsafe { next.read() };
                let digit = Engine::digit(token, 10);
                if digit >= 10 {
                    self.next = next;
                    *value = accumulated;
                    *digits = found;
                    return Ok(Some(token));
                }
                next = unsafe { next.add(1) };
                accumulated = match scan::append(accumulated, digit) {
                    Ok(value) => value,
                    Err(error) => {
                        self.next = next;
                        *value = accumulated;
                        *digits = found;
                        return Err(error);
                    }
                };
                found = true;
            }
            self.next = next;
            *value = accumulated;
            *digits = found;
            if self.window_end == self.limit || !self.refill() {
                return Ok(None);
            }
        }
    }
}
impl Frame {
    #[inline(always)]
    fn scope(&self) -> usize {
        // A native call drains its input before returning. The frame's address
        // identifies that live scope; this integer is never dereferenced.
        std::ptr::from_ref(self).addr()
    }

    #[inline(always)]
    pub(super) fn unread(&mut self, token: Token) {
        debug_assert!(self.back.is_none());
        self.back = Some(token);
    }
}
impl<const TRACE: bool> Producer<'_, TRACE> {
    #[inline(always)]
    pub(super) fn owns_pending(&self, input: &Frame) -> bool {
        self.pending
            .last()
            .is_some_and(|pending| pending.scope == input.scope())
    }

    #[inline(always)]
    pub(super) fn local(&mut self, input: &mut Frame) -> Result<Option<Token>> {
        if let Some(token) = input.back.take() {
            return Ok(Some(token));
        }
        if self.owns_pending(input)
            && let Some(token) = self.pending_token(input.scope())
        {
            return Ok(Some(token));
        }
        if let Some(body) = &mut input.body {
            return Ok(body.next());
        }
        let token = self.engine.file.read(
            self.engine.scalars.catcodes(),
            self.engine.scalars[Target::ENDLINE].value,
            &mut self.engine.names,
            self.engine.par,
        )?;
        if self.engine.names.len() > self.engine.bindings.len() {
            self.engine.grow_bindings(self.engine.names.len());
        }
        Ok(Some(token))
    }
    #[inline(never)]
    fn pending_token(&mut self, scope: usize) -> Option<Token> {
        while self
            .pending
            .last()
            .is_some_and(|pending| pending.scope == scope)
        {
            let token = match &mut self.pending.last_mut().unwrap().source {
                Source::Body(body) => body.next(),
                Source::Token(token) => {
                    let token = *token;
                    self.pending.pop();
                    return Some(token);
                }
                Source::Digits(bits) if *bits != 15 => {
                    let digit = (*bits & 15) as u8;
                    *bits >>= 4;
                    Some(Token::other(if digit == 10 { b'-' } else { b'0' + digit }))
                }
                Source::Digits(_) => None,
            };
            if token.is_some() {
                return token;
            }
            self.pending.pop();
        }
        None
    }
    #[inline(always)]
    pub(super) fn raw(&mut self, input: &mut Frame) -> Result<Read> {
        let token = if let Some(token) = self.local(input)? {
            token
        } else {
            return self.parent(input);
        };
        Ok(Read {
            token,
            meaning: self.engine.resolve(token),
        })
    }
    #[inline(never)]
    fn parent(&mut self, input: &mut Frame) -> Result<Read> {
        // SAFETY: produce/resume exclusively borrows the parent until this
        // child returns. Only the child uses this link, and pending inputs
        // never retain it. The root reads the file or errors, never reaching
        // this path with its null parent.
        let parent = unsafe { &mut *input.parent };
        let read = self.raw(parent)?;
        Ok(read)
    }
    pub(super) fn push_input(&mut self, input: &mut Frame, source: Source) -> Result<()> {
        if self.pending.len() + 1 + usize::from(input.back.is_some()) > 128 {
            return Err(Error::Capacity);
        }
        if let Some(token) = input.back.take() {
            self.pending.push(Pending {
                scope: input.scope(),
                source: Source::Token(token),
            });
        }
        self.pending.push(Pending {
            scope: input.scope(),
            source,
        });
        if TRACE {
            self.stats.max_pending_sources = self.stats.max_pending_sources.max(self.pending.len());
        }
        Ok(())
    }
    pub(super) fn decimal(&mut self, input: &mut Frame, value: i32) -> Result<()> {
        let mut magnitude = value.unsigned_abs();
        let mut bits = 15;
        loop {
            bits = (bits << 4) | u64::from(magnitude % 10);
            magnitude /= 10;
            if magnitude == 0 {
                break;
            }
        }
        if value < 0 {
            bits = (bits << 4) | 10;
        }
        self.push_input(input, Source::Digits(bits))
    }
    #[inline(always)]
    pub(super) fn take_literal<const KEYWORD: bool>(
        &mut self,
        input: &mut Frame,
        word: &[u8],
    ) -> bool {
        if input.back.is_some() {
            return false;
        }
        if self.owns_pending(input) {
            return match &mut self.pending.last_mut().unwrap().source {
                Source::Body(body) => body.take_literal::<KEYWORD>(word),
                _ => false,
            };
        }
        if let Some(body) = &mut input.body {
            return body.take_literal::<KEYWORD>(word);
        }
        self.engine
            .file
            .take_literal::<KEYWORD>(self.engine.scalars.catcodes(), word)
    }
    #[inline(always)]
    pub(super) fn literal_digits(
        &mut self,
        input: &mut Frame,
        value: &mut i32,
        digits: &mut bool,
    ) -> Result<Option<Token>> {
        if let Some(token) = input.back {
            return Ok(Some(token));
        }
        if self.owns_pending(input) {
            return self.pending_digits(value, digits);
        }
        if let Some(body) = &mut input.body {
            return body.digits(value, digits);
        }
        let mut accumulated = *value;
        while let Some(digit) = self
            .engine
            .file
            .literal_digit(self.engine.scalars.catcodes())
        {
            accumulated = match scan::append(accumulated, digit) {
                Ok(value) => value,
                Err(error) => {
                    *value = accumulated;
                    return Err(error);
                }
            };
            *digits = true;
        }
        *value = accumulated;
        Ok(self
            .engine
            .file
            .peek_character(self.engine.scalars.catcodes()))
    }
    #[inline(never)]
    fn pending_digits(&mut self, value: &mut i32, digits: &mut bool) -> Result<Option<Token>> {
        match &mut self.pending.last_mut().unwrap().source {
            Source::Body(body) => body.digits(value, digits),
            Source::Digits(bits) => {
                let mut rest = *bits;
                let mut accumulated = *value;
                while rest != 15 && rest & 15 < 10 {
                    let digit = (rest & 15) as i32;
                    rest >>= 4;
                    accumulated = match scan::append(accumulated, digit) {
                        Ok(value) => value,
                        Err(error) => {
                            *bits = rest;
                            *value = accumulated;
                            return Err(error);
                        }
                    };
                    *digits = true;
                }
                *bits = rest;
                *value = accumulated;
                Ok((rest != 15).then(|| Token::other(b'-')))
            }
            Source::Token(token) => Ok(Some(*token)),
        }
    }
}
