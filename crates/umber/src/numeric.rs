//! TeX scaled arithmetic and inline glue values; see tex.web sections 102–109.
use crate::{Error, Result};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Scaled(pub i32);
impl Scaled {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(65536);
    pub const MAX: Self = Self(0x3fff_ffff);
    pub fn checked(value: i64) -> Result<Self> {
        if value < -i64::from(Self::MAX.0) || value > i64::from(Self::MAX.0) {
            Err(Error::Overflow)
        } else {
            Ok(Self(value as i32))
        }
    }
    // Register addition in TeX is a word addition, unlike nx_plus_y used for
    // multiplication. Values above max_dimen can therefore exist in registers.
    pub fn advance(self, other: Self) -> Self {
        Self(self.0.wrapping_add(other.0))
    }
    pub fn multiply(self, n: i32) -> Result<Self> {
        Self::checked(i64::from(self.0) * i64::from(n))
    }
    pub fn divide(self, n: i32) -> Result<Self> {
        self.0.checked_div(n).map(Self).ok_or(Error::Overflow)
    }
    pub fn negate(self) -> Self {
        Self(self.0.wrapping_neg())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum GlueOrder {
    #[default]
    Normal,
    Fil,
    Fill,
    Filll,
}
impl GlueOrder {
    pub fn suffix(self) -> &'static [u8] {
        match self {
            Self::Normal => b"pt",
            Self::Fil => b"fil",
            Self::Fill => b"fill",
            Self::Filll => b"filll",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Glue {
    pub width: Scaled,
    pub stretch: Scaled,
    pub shrink: Scaled,
    pub stretch_order: GlueOrder,
    pub shrink_order: GlueOrder,
}
impl Glue {
    pub fn canonical(self) -> Self {
        if self.width.0 == 0 && self.stretch.0 == 0 && self.shrink.0 == 0 {
            Self::default()
        } else {
            self
        }
    }
    pub fn negate(mut self) -> Self {
        self.width = self.width.negate();
        self.stretch = self.stretch.negate();
        self.shrink = self.shrink.negate();
        self
    }
    pub fn advance(self, mut operand: Self) -> Self {
        fn component(q: &mut Scaled, order: &mut GlueOrder, r: Scaled, ro: GlueOrder) {
            if q.0 == 0 {
                *order = GlueOrder::Normal;
            }
            if *order == ro {
                *q = q.advance(r);
            } else if *order < ro && r.0 != 0 {
                *q = r;
                *order = ro;
            }
        }
        operand.width = operand.width.advance(self.width);
        component(
            &mut operand.stretch,
            &mut operand.stretch_order,
            self.stretch,
            self.stretch_order,
        );
        component(
            &mut operand.shrink,
            &mut operand.shrink_order,
            self.shrink,
            self.shrink_order,
        );
        operand.canonical()
    }
    pub fn arithmetic(mut self, n: i32, divide: bool) -> Result<Self> {
        for amount in [&mut self.width, &mut self.stretch, &mut self.shrink] {
            *amount = if divide {
                amount.divide(n)?
            } else {
                amount.multiply(n)?
            };
        }
        Ok(self.canonical())
    }
}

pub(crate) fn round_decimals(digits: &[u8]) -> i32 {
    let mut a = 0;
    for &digit in digits.iter().rev() {
        a = (a + i32::from(digit) * 131072) / 10;
    }
    (a + 1) / 2
}

/// TeX print_scaled; the caller chooses the destination and token categories.
pub(crate) fn print_scaled(value: Scaled, mut emit: impl FnMut(u8)) {
    let mut s = i64::from(value.0);
    if s < 0 {
        emit(b'-');
        s = -s;
    }
    let mut number = [0; 10];
    let mut length = 0;
    let mut integer = s / 65536;
    loop {
        number[length] = b'0' + (integer % 10) as u8;
        length += 1;
        integer /= 10;
        if integer == 0 {
            break;
        }
    }
    for &byte in number[..length].iter().rev() {
        emit(byte);
    }
    emit(b'.');
    s = 10 * (s % 65536) + 5;
    let mut delta = 10;
    loop {
        if delta > 65536 {
            s += 32768 - 50000;
        }
        emit(b'0' + (s / 65536) as u8);
        s = 10 * (s % 65536);
        delta *= 10;
        if s <= delta {
            break;
        }
    }
}

/// The exact integer heuristic used by try_break and vertical breaking.
#[allow(dead_code)] // Consumed by the subsequent paragraph/page implementation.
pub(crate) fn badness(t: Scaled, s: Scaled) -> i32 {
    if t.0 == 0 {
        return 0;
    }
    if s.0 <= 0 {
        return 10000;
    }
    let r = if t.0 <= 7_230_584 {
        i64::from(t.0) * 297 / i64::from(s.0)
    } else if s.0 >= 1_663_497 {
        i64::from(t.0 / (s.0 / 297))
    } else {
        i64::from(t.0)
    };
    if r > 1290 {
        10000
    } else {
        ((r * r * r + 131072) / 262144) as i32
    }
}

#[cfg(test)]
#[path = "numeric_tests.rs"]
mod tests;
