use crate::{Engine, Error, Result, token::*};

impl Engine<'_> {
    fn expression_range(value: i64, overflow: &mut bool) -> i32 {
        if value.abs() > i64::from(i32::MAX) {
            *overflow = true;
            0
        } else {
            value as i32
        }
    }
    fn rounded_divide(numerator: i64, denominator: i32, overflow: &mut bool) -> i32 {
        if denominator == 0 {
            *overflow = true;
            return 0;
        }
        let negative = (numerator < 0) ^ (denominator < 0);
        let n = numerator.unsigned_abs();
        let d = u64::from(denominator.unsigned_abs());
        let value = n / d + u64::from((n % d) * 2 >= d);
        Self::expression_range(
            if negative {
                -(value as i64)
            } else {
                value as i64
            },
            overflow,
        )
    }
    fn expression_factor(&mut self, depth: u16, overflow: &mut bool) -> Result<i32> {
        let read = self.nonspace(true)?;
        if read.token == Token::other(b'(') {
            if depth >= 256 {
                return Err(Error::Capacity);
            }
            self.expression_sum(depth + 1, true, overflow)
        } else if (Token::other(b'0').0..=Token::other(b'9').0).contains(&read.token.0) {
            self.scan_digits::<10>(i32::from(read.token.byte() - b'0'))
        } else {
            self.back(read.token);
            self.scan_int()
        }
    }
    fn expression_term(&mut self, depth: u16, overflow: &mut bool) -> Result<(i32, Read)> {
        let first = self.expression_factor(depth, overflow)?;
        let mut operator = self.nonspace(true)?;
        let mut value = Self::expression_range(i64::from(first), overflow);
        loop {
            if operator.token == Token::other(b'*') {
                let factor = self.expression_factor(depth, overflow)?;
                operator = self.nonspace(true)?;
                let factor = Self::expression_range(i64::from(factor), overflow);
                let product = i64::from(value) * i64::from(factor);
                if operator.token == Token::other(b'/') {
                    let divisor = self.expression_factor(depth, overflow)?;
                    operator = self.nonspace(true)?;
                    let divisor = Self::expression_range(i64::from(divisor), overflow);
                    value = Self::rounded_divide(product, divisor, overflow);
                } else {
                    value = Self::expression_range(product, overflow);
                }
            } else if operator.token == Token::other(b'/') {
                let divisor = self.expression_factor(depth, overflow)?;
                operator = self.nonspace(true)?;
                let divisor = Self::expression_range(i64::from(divisor), overflow);
                value = Self::rounded_divide(i64::from(value), divisor, overflow);
            } else {
                return Ok((value, operator));
            }
        }
    }
    fn expression_sum(
        &mut self,
        depth: u16,
        parenthesized: bool,
        overflow: &mut bool,
    ) -> Result<i32> {
        let (mut value, mut operator) = self.expression_term(depth, overflow)?;
        while matches!(operator.token, t if t == Token::other(b'+') || t == Token::other(b'-')) {
            let subtract = operator.token == Token::other(b'-');
            let (term, next) = self.expression_term(depth, overflow)?;
            value = Self::expression_range(
                i64::from(value)
                    + if subtract {
                        -i64::from(term)
                    } else {
                        i64::from(term)
                    },
                overflow,
            );
            operator = next;
        }
        if parenthesized {
            if operator.token != Token::other(b')') {
                return Err(Error::Syntax);
            }
        } else if operator.meaning.command != cmd::RELAX {
            self.back(operator.token);
        }
        Ok(value)
    }
    pub(crate) fn scan_expression(&mut self) -> Result<i32> {
        // e-TeX keeps scanning after an arithmetic failure, substituting zero
        // for the failing intermediate. Report overflow only at expression end:
        // a later scan error (such as an undefined control sequence) comes first.
        self.with_expansion(|engine| {
            let mut overflow = false;
            let value = engine.expression_sum(0, false, &mut overflow)?;
            if overflow {
                Err(Error::Overflow)
            } else {
                Ok(value)
            }
        })
    }
}
