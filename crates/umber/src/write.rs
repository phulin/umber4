//! Immediate diagnostic writes; deferred and file/shell output stay unsupported.
use crate::{
    Diagnostic, DiagnosticDestination, Engine, Error, Result, scanner::Scanner, text::Mode,
    token::*, values::Class,
};

const WRITE: i32 = 1;
const IMMEDIATE: i32 = 4;

impl Engine<'_> {
    pub(crate) fn extension(&mut self, operation: i32) -> Result<()> {
        if operation != IMMEDIATE {
            return Err(Error::Unsupported);
        }
        let read = self.next_expanded()?;
        if read.meaning.command == cmd::EXTENSION && read.meaning.value <= 2 {
            if read.meaning.value != WRITE {
                return Err(Error::Unsupported);
            }
            self.immediate_write()
        } else {
            // IMMEDIATE's one expanded lookahead consumed suppression already.
            self.back(read.token);
            Ok(())
        }
    }
    fn immediate_write(&mut self) -> Result<()> {
        let stream = self.scan_int()?;
        let mut tokens = self.values.builder(Class::Transient)?;
        self.scan_raw_text(&mut tokens)?;
        if stream == 18 {
            return Err(Error::Unsupported);
        }
        let tokens = tokens.finish();
        let source_base = self.document.sources.len();
        let old_mode = std::mem::replace(&mut self.document.text.mode, Mode::Write);
        let result = (|| {
            self.check_source_limit()?;
            self.document
                .sources
                .push(Source::WriteBoundary { position: 0 });
            self.push_tokens(tokens, 0)?;
            self.back(Token::character(1, b'{'));
            let mut bytes = Vec::new();
            self.with_scanner(Scanner::Absorbing, |engine| {
                engine.opening_brace()?;
                engine.scan_body::<true>(None, &mut |engine: &mut Self, token: Token| {
                    engine.print_token::<false>(token, &mut bytes);
                    Ok(())
                })?;
                Ok(())
            })?;
            if self.next_token()?.token != FROZEN_END_WRITE {
                return Err(Error::Syntax);
            }
            Ok(bytes)
        })();
        while self.document.sources.len() > source_base {
            self.pop_source();
        }
        self.document.text.mode = old_mode;
        let bytes = result?;
        self.document.diagnostics.push(Diagnostic::Write {
            destination: if stream < 0 {
                DiagnosticDestination::LogOnly
            } else {
                DiagnosticDestination::TerminalAndLog
            },
            bytes,
        });
        Ok(())
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
