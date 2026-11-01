//! Selective native preamble copying into one owned immutable token value.
use super::*;

impl Engine<'_> {
    fn preamble_token(&mut self) -> Result<Read> {
        loop {
            let mut read = self.next_token()?;
            while read.meaning.command == 4 && read.meaning.value == 256 {
                read = self.next_token()?;
                if read.meaning.expandable() {
                    if read.meaning.is_macro() {
                        self.begin_macro(read.meaning.value as u32)?;
                    } else {
                        self.expand(read)?;
                    }
                    read = self.next_token()?;
                }
            }
            if read.meaning.command == cmd::ENDV {
                return Err(Error::Syntax);
            }
            if read.meaning.command == cmd::GLUE_PARAM
                && read.meaning.value == GlueParam::TabSkip as i32
            {
                self.scan_equals()?;
                let glue = self.scan_glue()?;
                let global = self.document.scalars[crate::scalars::Target::GLOBALDEFS].value > 0;
                self.assign_glue(GlueParam::TabSkip.index(), glue, global);
                continue;
            }
            return Ok(read);
        }
    }
    fn preamble_len(&self) -> u32 {
        self.document
            .alignment_frames
            .last()
            .unwrap()
            .preamble
            .as_ref()
            .unwrap()
            .len()
    }
    fn preamble_push(&mut self, token: Token) -> Result<()> {
        self.document
            .alignment_frames
            .last_mut()
            .unwrap()
            .preamble
            .as_mut()
            .unwrap()
            .push(token)
    }
    pub(super) fn scan_alignment_preamble(&mut self) -> Result<()> {
        self.document.alignment_frames.last_mut().unwrap().leading =
            self.document.layout_values.glues[GlueParam::TabSkip.index()].value;
        loop {
            let start = self.preamble_len();
            loop {
                let read = self.preamble_token()?;
                if read.meaning.command == 6 {
                    break;
                }
                if matches!(read.meaning.command, 4 | cmd::ALIGN_CR)
                    && self.alignment_reader.state == -1000000
                {
                    let frame = self.document.alignment_frames.last_mut().unwrap();
                    if frame.preamble.as_ref().unwrap().len() == start
                        && frame.period.is_none()
                        && read.meaning.command == 4
                    {
                        frame.period = Some((frame.columns.len(), 0));
                        continue;
                    }
                    self.back(read.token)?;
                    return Err(Error::Syntax);
                }
                if read.meaning.command != cmd::SPACE || self.preamble_len() != start {
                    self.preamble_push(read.token)?;
                }
            }
            let split = self.preamble_len();
            let terminator = loop {
                let read = self.preamble_token()?;
                if matches!(read.meaning.command, 4 | cmd::ALIGN_CR)
                    && self.alignment_reader.state == -1000000
                {
                    break read;
                }
                if read.meaning.command == 6 {
                    return Err(Error::Syntax);
                }
                self.preamble_push(read.token)?;
            };
            self.preamble_push(FROZEN_END_TEMPLATE)?;
            let end = self.preamble_len();
            let frame = self.document.alignment_frames.last_mut().unwrap();
            let store = frame.preamble.as_ref().unwrap().id();
            let after = self.document.layout_values.glues[GlueParam::TabSkip.index()].value;
            frame.columns.push(ColumnTemplate {
                u: Span {
                    store,
                    start,
                    end: split,
                },
                v: Span {
                    store,
                    start: split,
                    end,
                },
                after,
            });
            if terminator.meaning.command == cmd::ALIGN_CR {
                if let Some((_, end)) = &mut frame.period {
                    *end = frame.columns.len();
                }
                return Ok(());
            }
        }
    }
}
