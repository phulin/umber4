//! Wire identities are checked before runtime dispatch can index their payloads.
use crate::{Error, Result, token::*};
pub(super) fn token(token: Token, names: usize, fonts: usize, parameter: bool) -> Result<()> {
    let valid = if let Some(font) = token.font_id() {
        font < fonts
    } else if token.is_cs() {
        token.0 < 2 * CS && token.id() < names
    } else if token.is_parameter() {
        parameter && token.0 & !255 == PARAM
    } else {
        token.0 < 16 * 256
    };
    if valid { Ok(()) } else { Err(Error::Format) }
}
pub(super) fn meaning(m: Meaning, values: usize, fonts: usize) -> Result<()> {
    let valid = if m.is_macro() {
        m.value >= 0 && (m.value as usize) < values
    } else {
        match m.command {
            0..=15 => {
                (0..=255).contains(&m.value)
                    || (m.command == 4 && m.value == 256)
                    || (m.command == 5 && (257..=258).contains(&m.value))
            }
            cmd::COUNT_REF
            | cmd::TOKS_REF
            | cmd::DIMEN_REF
            | cmd::SKIP_REF
            | cmd::MUSKIP_REF
            | cmd::CHAR_GIVEN => (0..=255).contains(&m.value),
            cmd::MATH_GIVEN => (0..=32767).contains(&m.value),
            cmd::SELECT_FONT => m.value >= 0 && (m.value as usize) < fonts,
            cmd::UNDEFINED | cmd::END_WRITE | cmd::END_TEMPLATE => m.value == 0,
            cmd::RELAX => m.value == 0 || m.value == 257,
            _ => (0..cmd::names(m.command).len() as i32).contains(&m.value),
        }
    };
    if valid { Ok(()) } else { Err(Error::Format) }
}
