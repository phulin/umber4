//! TeX's expanded, normal-context pattern and exception readers.
use crate::{Engine, Error, Result, scalars::Target, token::*};

impl Engine<'_> {
    fn hyphenation_language(&self) -> u8 {
        let language = self.document.scalars[Target::LANGUAGE].value;
        if (1..=255).contains(&language) {
            language as u8
        } else {
            0
        }
    }
    pub(crate) fn hyphenation_assignment(&mut self, patterns: bool) -> Result<()> {
        if patterns {
            self.document.hyphenation.check_patterns()?;
            let language = self.hyphenation_language();
            self.opening_brace()?;
            self.scan_patterns(language)
        } else {
            self.opening_brace()?;
            let language = self.hyphenation_language();
            self.scan_exceptions(language)
        }
    }
    fn scan_patterns(&mut self, language: u8) -> Result<()> {
        let mut letters = [0; 63];
        let mut weights = [0; 64];
        let mut length = 0;
        let mut digit = false;
        loop {
            let read = self.next_expanded()?;
            match read.meaning.command {
                11 | 12 => {
                    let character = read.meaning.value as u8;
                    if !digit && character.is_ascii_digit() {
                        if length < 63 {
                            weights[length] = character - b'0';
                            digit = true;
                        }
                    } else {
                        let lowercase = if character == b'.' {
                            0
                        } else {
                            let code = self.document.scalars
                                [Target::casecode(0, character as usize)]
                            .value;
                            if code == 0 {
                                return Err(Error::Syntax);
                            }
                            code as u8
                        };
                        if length < 63 {
                            letters[length] = lowercase;
                            length += 1;
                            weights[length] = 0;
                            digit = false;
                        }
                    }
                }
                cmd::SPACE | cmd::END_GROUP_CHAR => {
                    if length != 0 {
                        if letters[0] == 0 {
                            weights[0] = 0;
                        }
                        if letters[length - 1] == 0 {
                            weights[length] = 0;
                        }
                        self.document.hyphenation.pattern(
                            language,
                            &letters[..length],
                            &weights[..=length],
                        )?;
                    }
                    if read.meaning.command == cmd::END_GROUP_CHAR {
                        return Ok(());
                    }
                    length = 0;
                    weights[0] = 0;
                    digit = false;
                }
                _ => return Err(Error::Syntax),
            }
        }
    }
    fn scan_exceptions(&mut self, language: u8) -> Result<()> {
        let mut letters = [0; 63];
        let mut length = 0;
        let mut positions = 0u64;
        loop {
            let read = self.next_expanded()?;
            match read.meaning.command {
                11 | 12 | cmd::CHAR_GIVEN | cmd::CHAR => {
                    let character = if read.meaning.command == cmd::CHAR {
                        self.scan_index()? as u8
                    } else {
                        read.meaning.value as u8
                    };
                    if character == b'-' {
                        if length < 63 {
                            positions |= 1u64 << length;
                        }
                    } else {
                        let code =
                            self.document.scalars[Target::casecode(0, character as usize)].value;
                        if code == 0 {
                            return Err(Error::Syntax);
                        }
                        if length < 63 {
                            letters[length] = code as u8;
                            length += 1;
                        }
                    }
                }
                cmd::SPACE | cmd::END_GROUP_CHAR => {
                    if length > 1 {
                        self.document.hyphenation.exception(
                            language,
                            &letters[..length],
                            positions,
                        )?;
                    }
                    if read.meaning.command == cmd::END_GROUP_CHAR {
                        return Ok(());
                    }
                    length = 0;
                    positions = 0;
                }
                _ => return Err(Error::Syntax),
            }
        }
    }
}
