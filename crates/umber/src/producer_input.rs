use super::*;
impl Body {
    #[inline(always)]
    pub(super) fn new(owner: values::Owner) -> Self {
        let (next, end, window_end, limit) = owner.body_window();
        Self {
            owner,
            next,
            end,
            window_end,
            limit,
        }
    }
    #[inline(always)]
    pub(super) fn restart(&mut self) {
        (self.next, self.end, self.window_end, self.limit) = self.owner.body_window();
    }
    #[inline(never)]
    pub(super) fn refill(&mut self) -> bool {
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
    pub(super) fn skip_inert(&mut self, resolve: impl Fn(Token) -> Meaning) {
        let mut next = self.next;
        while next != self.end {
            // SAFETY: the owner retains this immutable initialized window.
            // Meaning lookup cannot modify it, and the first active token stays
            // unread for the ordinary dispatcher.
            let token = unsafe { next.read() };
            if !matches!(resolve(token).command, cmd::RELAX | cmd::SPACE | cmd::PAR) {
                break;
            }
            next = unsafe { next.add(1) };
        }
        self.next = next;
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
            return self.backed_literal::<KEYWORD>(input, word);
        }
        self.literal_tail::<KEYWORD>(input, word)
    }
    #[inline(never)]
    fn backed_literal<const KEYWORD: bool>(&mut self, input: &mut Frame, word: &[u8]) -> bool {
        let token = input.back.unwrap();
        let matches = if KEYWORD {
            token.cat() != 10 && !token.is_cs() && token.byte().eq_ignore_ascii_case(&word[0])
        } else {
            token == Token::other(word[0])
        };
        if matches && (word.len() == 1 || self.literal_tail::<KEYWORD>(input, &word[1..])) {
            input.back = None;
            true
        } else {
            false
        }
    }
    #[inline(always)]
    fn literal_tail<const KEYWORD: bool>(&mut self, input: &mut Frame, word: &[u8]) -> bool {
        if self.owns_pending(input) {
            match &mut self.pending.last_mut().unwrap().source {
                Source::Body(body) => body.take_literal::<KEYWORD>(word),
                _ => false,
            }
        } else if let Some(body) = &mut input.body {
            body.take_literal::<KEYWORD>(word)
        } else {
            self.engine
                .file
                .take_literal::<KEYWORD>(self.engine.scalars.catcodes(), word)
        }
    }
    #[inline(always)]
    // None completes the number. A returned Read is already consumed; input
    // exhaustion continues through raw, including borrowed callers.
    pub(super) fn integer_token(
        &mut self,
        input: &mut Frame,
        value: &mut i32,
        digits: &mut bool,
    ) -> Result<Option<Read>> {
        if let Some(token) = input.back {
            let read = Read {
                token,
                meaning: self.engine.resolve(token),
            };
            return Ok(observed(read, *digits, || {
                input.back = None;
            }));
        }
        let pending = self.owns_pending(input);
        let body = if pending {
            match &mut self.pending.last_mut().unwrap().source {
                Source::Body(body) => Some(body),
                _ => None,
            }
        } else {
            input.body.as_mut()
        };
        if let Some(body) = body {
            if let Some(token) = body.digits(value, digits)? {
                let read = Read {
                    token,
                    meaning: self.engine.resolve(token),
                };
                return Ok(observed(read, *digits, || {
                    // SAFETY: digits observed this initialized window token.
                    body.next = unsafe { body.next.add(1) };
                }));
            }
        } else if pending {
            match &mut self.pending.last_mut().unwrap().source {
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
                    if rest != 15 {
                        let token = Token::other(b'-');
                        let read = Read {
                            token,
                            meaning: self.engine.resolve(token),
                        };
                        return Ok(observed(read, *digits, || {
                            *bits >>= 4;
                        }));
                    }
                }
                Source::Token(token) => {
                    let read = Read {
                        token: *token,
                        meaning: self.engine.resolve(*token),
                    };
                    return Ok(observed(read, *digits, || {
                        self.pending.pop();
                    }));
                }
                Source::Body(_) => unreachable!(),
            }
        } else {
            if let Some(token) =
                self.engine
                    .file
                    .literal_digits(self.engine.scalars.catcodes(), value, digits)?
            {
                let read = Read {
                    token,
                    meaning: self.engine.resolve(token),
                };
                // Source lookahead fixes the token's category before an
                // assignment can change it. Stored ranges already do this.
                self.engine.file.consume_character();
                if *digits {
                    input.unread(token);
                    return Ok(None);
                }
                return Ok(Some(read));
            }
        }
        self.raw(input).map(Some)
    }
}
// A batched digit scan has already observed this token in the selected input.
// Leave a nonspace numeric terminator there; otherwise consume it through the
// same input, without a second route lookup or meaning resolution.
#[inline(always)]
fn observed(read: Read, digits: bool, consume: impl FnOnce()) -> Option<Read> {
    if digits && !read.meaning.expandable() {
        if read.meaning.command == cmd::SPACE {
            consume();
        }
        None
    } else {
        consume();
        Some(read)
    }
}
