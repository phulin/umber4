//! Dimension and glue operand scanning, preserving TeX's integer rounding order.
use crate::{
    Engine, Error, Result,
    layout_values::REGISTERS,
    numeric::{Glue, GlueOrder, Scaled, round_decimals},
    quantities::Quantity,
    scalars::Target,
    token::*,
};

impl Engine<'_> {
    pub(crate) fn dimen_target(&mut self, read: Read) -> Result<usize> {
        match read.meaning.command {
            cmd::DIMEN => self.scan_index(),
            cmd::DIMEN_REF => Ok(read.meaning.value as usize),
            cmd::DIMEN_PARAM => Ok(REGISTERS + read.meaning.value as usize),
            _ => Err(Error::Number),
        }
    }
    pub(crate) fn glue_target(&mut self, read: Read) -> Result<usize> {
        match read.meaning.command {
            cmd::SKIP => self.scan_index(),
            cmd::SKIP_REF => Ok(read.meaning.value as usize),
            cmd::GLUE_PARAM => Ok(REGISTERS + read.meaning.value as usize),
            _ => Err(Error::Number),
        }
    }
    pub(crate) fn scan_signs(&mut self) -> Result<(Read, bool)> {
        let mut negative = false;
        loop {
            let read = self.nonspace(true)?;
            if read.token == Token::other(b'-') {
                negative = !negative;
            } else if read.token != Token::other(b'+') {
                return Ok((read, negative));
            }
        }
    }
    pub(crate) fn scan_dimen(&mut self) -> Result<Scaled> {
        let (read, negative) = self.scan_signs()?;
        Ok(self.dimension_from(read, negative, false)?.0)
    }
    fn dimension_from(
        &mut self,
        read: Read,
        negative: bool,
        infinite: bool,
    ) -> Result<(Scaled, GlueOrder)> {
        if let Some(value) = self.quantity(read)? {
            return match value {
                Quantity::Dimension(v) => Ok((
                    self.signed_dimension(i64::from(v.0), negative)?,
                    GlueOrder::Normal,
                )),
                Quantity::Glue(g) => Ok((
                    self.signed_dimension(i64::from(g.width.0), negative)?,
                    GlueOrder::Normal,
                )),
                Quantity::Integer(v) => self.dimension_units(i64::from(v), 0, negative, infinite),
            };
        }
        let mut integer = 0i64;
        let mut fraction = 0;
        let mut next = read;
        if (Token::other(b'0').0..=Token::other(b'9').0).contains(&read.token.0) {
            loop {
                integer = integer
                    .checked_mul(10)
                    .and_then(|v| v.checked_add(i64::from(next.token.byte() - b'0')))
                    .filter(|&v| v <= i64::from(i32::MAX))
                    .ok_or(Error::Overflow)?;
                next = self.next_expanded()?;
                if !(Token::other(b'0').0..=Token::other(b'9').0).contains(&next.token.0) {
                    break;
                }
            }
        } else if !matches!(read.token, t if t == Token::other(b'.') || t == Token::other(b',')) {
            self.back(read.token);
            integer = i64::from(self.scan_int()?);
            return self.dimension_units(integer, 0, negative, infinite);
        }
        if next.token == Token::other(b'.') || next.token == Token::other(b',') {
            let mut digits = [0; 17];
            let mut len = 0;
            loop {
                next = self.next_expanded()?;
                if !(Token::other(b'0').0..=Token::other(b'9').0).contains(&next.token.0) {
                    break;
                }
                if len < digits.len() {
                    digits[len] = next.token.byte() - b'0';
                    len += 1;
                }
            }
            fraction = round_decimals(&digits[..len]);
        }
        if next.meaning.command != cmd::SPACE {
            self.back(next.token);
        }
        self.dimension_units(integer, fraction, negative, infinite)
    }
    fn signed_dimension(&self, value: i64, negative: bool) -> Result<Scaled> {
        Scaled::checked(if negative { -value } else { value })
    }
    fn dimension_units(
        &mut self,
        mut integer: i64,
        mut fraction: i32,
        mut negative: bool,
        infinite: bool,
    ) -> Result<(Scaled, GlueOrder)> {
        if integer < 0 {
            integer = -integer;
            negative = !negative;
        }
        let mut order = GlueOrder::Normal;
        if infinite && self.scan_keyword(b"fil")? {
            order = GlueOrder::Fil;
            while self.scan_keyword(b"l")? {
                order = match order {
                    GlueOrder::Fil => GlueOrder::Fill,
                    GlueOrder::Fill => GlueOrder::Filll,
                    _ => return Err(Error::Number),
                };
            }
        } else {
            let read = self.nonspace(true)?;
            if let Some(value) = self.quantity(read)? {
                let unit = match value {
                    Quantity::Integer(v) => v,
                    Quantity::Dimension(v) => v.0,
                    Quantity::Glue(g) => g.width.0,
                };
                let value =
                    integer * i64::from(unit) + i64::from(unit) * i64::from(fraction) / 65536;
                return Ok((self.signed_dimension(value, negative)?, order));
            }
            self.back(read.token);
            let font_unit = if self.scan_keyword(b"em")? {
                Some(self.font_unit(false))
            } else if self.scan_keyword(b"ex")? {
                Some(self.font_unit(true))
            } else {
                None
            };
            if let Some(unit) = font_unit {
                self.dimension_space()?;
                let value =
                    integer * i64::from(unit.0) + i64::from(unit.0) * i64::from(fraction) / 65536;
                return Ok((self.signed_dimension(value, negative)?, order));
            }
            if self.scan_keyword(b"true")? {
                let mag = self.document.scalars[Target::MAG].value;
                if !(1..=32768).contains(&mag)
                    || self
                        .document
                        .layout_values
                        .prepared_mag
                        .is_some_and(|old| old != mag)
                {
                    return Err(Error::Number);
                }
                self.document.layout_values.prepared_mag = Some(mag);
                Self::convert_unit(&mut integer, &mut fraction, 1000, i64::from(mag));
            }
            if !self.scan_keyword(b"pt")? {
                let mut ratio = None;
                for (name, num, den) in [
                    (b"in", 7227, 100),
                    (b"pc", 12, 1),
                    (b"cm", 7227, 254),
                    (b"mm", 7227, 2540),
                    (b"bp", 7227, 7200),
                    (b"dd", 1238, 1157),
                    (b"cc", 14856, 1157),
                ] {
                    if self.scan_keyword(name)? {
                        ratio = Some((num, den));
                        break;
                    }
                }
                if let Some((num, den)) = ratio {
                    Self::convert_unit(&mut integer, &mut fraction, num, den);
                } else if self.scan_keyword(b"sp")? {
                    self.dimension_space()?;
                    return Ok((self.signed_dimension(integer, negative)?, order));
                } else {
                    return Err(Error::Number);
                }
            }
        }
        self.dimension_space()?;
        Ok((
            self.signed_dimension(
                integer
                    .checked_mul(i64::from(Scaled::ONE.0))
                    .and_then(|value| value.checked_add(i64::from(fraction)))
                    .ok_or(Error::Overflow)?,
                negative,
            )?,
            order,
        ))
    }
    fn convert_unit(integer: &mut i64, fraction: &mut i32, num: i64, den: i64) {
        let product = *integer * num;
        let f = (num * i64::from(*fraction) + 65536 * (product % den)) / den;
        *integer = product / den + f / 65536;
        *fraction = (f % 65536) as i32;
    }
    fn dimension_space(&mut self) -> Result<()> {
        let read = self.next_expanded()?;
        if read.meaning.command != cmd::SPACE {
            self.back(read.token);
        }
        Ok(())
    }
    pub(crate) fn scan_glue(&mut self) -> Result<Glue> {
        let (read, negative) = self.scan_signs()?;
        let width = match self.quantity(read)? {
            Some(Quantity::Glue(g)) => return Ok(if negative { g.negate() } else { g }),
            // Unlike scan_dimen, scan_glue does not range-check a direct
            // internal dimension; register addition may have exceeded max_dimen.
            Some(Quantity::Dimension(v)) => {
                if negative {
                    v.negate()
                } else {
                    v
                }
            }
            Some(Quantity::Integer(v)) => self.dimension_units(i64::from(v), 0, negative, false)?.0,
            None => self.dimension_from(read, negative, false)?.0,
        };
        let mut glue = Glue {
            width,
            ..Glue::default()
        };
        if self.scan_keyword(b"plus")? {
            let (read, negative) = self.scan_signs()?;
            (glue.stretch, glue.stretch_order) = self.dimension_from(read, negative, true)?;
        }
        if self.scan_keyword(b"minus")? {
            let (read, negative) = self.scan_signs()?;
            (glue.shrink, glue.shrink_order) = self.dimension_from(read, negative, true)?;
        }
        Ok(glue.canonical())
    }
}
