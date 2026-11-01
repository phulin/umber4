use crate::token::{Integer, cmd};

const REGISTERS: usize = 256;
const CAT_BASE: usize = REGISTERS;
const ENDLINE: usize = CAT_BASE + 256;
const PARAM_BASE: usize = ENDLINE + 1;
const PARAMS: usize = cmd::names(cmd::INTEGER_PARAM).len();
const CASE_BASE: usize = PARAM_BASE + PARAMS;
const SF_BASE: usize = CASE_BASE + 2 * 256;
const MATH_BASE: usize = SF_BASE + 256;
const DEL_BASE: usize = MATH_BASE + 256;
const SLOTS: usize = DEL_BASE + 256;
use std::ops::{Index, IndexMut};

// Scalar values have the same representation and group-save lifetime. A target
// identifies a slot directly, so assignment need not dispatch separately for
// reading, saving, and writing each kind of scalar table.
#[derive(Clone, Copy)]
pub(crate) struct Target(u16);
impl Target {
    pub const ENDLINE: Self = Self(ENDLINE as u16);
    pub const GLOBALDEFS: Self = Self(PARAM_BASE as u16);
    pub const ESCAPECHAR: Self = Self((PARAM_BASE + 1) as u16);
    pub const MAG: Self = Self((PARAM_BASE + 6) as u16);
    pub const DEFAULT_HYPHEN: Self = Self((PARAM_BASE + 7) as u16);
    pub const DEFAULT_SKEW: Self = Self((PARAM_BASE + 8) as u16);
    pub fn paragraph(parameter: ParagraphParam) -> Self {
        Self::parameter(9 + parameter as usize)
    }
    pub fn sfcode(index: usize) -> Self {
        Self((SF_BASE + index) as u16)
    }
    pub fn mathcode(index: usize) -> Self {
        Self((MATH_BASE + index) as u16)
    }
    pub fn delcode(index: usize) -> Self {
        Self((DEL_BASE + index) as u16)
    }
    pub fn count(index: usize) -> Self {
        Self(index as u16)
    }
    pub fn catcode(index: usize) -> Self {
        Self((CAT_BASE + index) as u16)
    }
    pub fn parameter(index: usize) -> Self {
        Self((PARAM_BASE + index) as u16)
    }
    pub fn casecode(which: usize, index: usize) -> Self {
        Self((CASE_BASE + which * 256 + index) as u16)
    }
}

#[derive(Clone, Copy)]
#[repr(usize)]
pub(crate) enum ParagraphParam {
    Pretolerance,
    Tolerance,
    LinePenalty,
    HyphenPenalty,
    ExHyphenPenalty,
    AdjDemerits,
    DoubleHyphenDemerits,
    FinalHyphenDemerits,
    Looseness,
    HangAfter,
    InterlinePenalty,
    ClubPenalty,
    WidowPenalty,
    BrokenPenalty,
}
#[derive(Clone)]
pub(crate) struct Scalars([Integer; SLOTS]);
impl Default for Scalars {
    fn default() -> Self {
        let mut state = Self([Integer::default(); SLOTS]);
        state.0[CAT_BASE..ENDLINE].fill(Integer {
            value: 12,
            level: 0,
        });
        state[Target::ENDLINE].value = 13;
        for (index, value) in [0, 92, 0, 1, 1, 1970, 1000, 0, 0].into_iter().enumerate() {
            state[Target::parameter(index)].value = value;
        }
        state[Target::paragraph(ParagraphParam::Tolerance)].value = 10000;
        state[Target::paragraph(ParagraphParam::HangAfter)].value = 1;
        state.0[SF_BASE..MATH_BASE].fill(Integer {
            value: 1000,
            level: 0,
        });
        for byte in 0..=255 {
            state[Target::mathcode(byte)].value = byte as i32;
            state[Target::delcode(byte)].value = -1;
        }
        state[Target::delcode(b'.' as usize)].value = 0;
        for byte in b'0'..=b'9' {
            state[Target::mathcode(byte as usize)].value += 0x7000;
        }
        for byte in b'A'..=b'Z' {
            state[Target::sfcode(byte as usize)].value = 999;
        }
        for byte in b'a'..=b'z' {
            for c in [byte, byte.to_ascii_uppercase()] {
                state[Target::catcode(c as usize)].value = 11;
                state[Target::casecode(0, c as usize)].value = i32::from(byte);
                state[Target::casecode(1, c as usize)].value = i32::from(byte.to_ascii_uppercase());
                state[Target::mathcode(c as usize)].value += 0x7100;
            }
        }
        for (byte, cat) in [
            (b'\\', 0),
            (b' ', 10),
            (13, 5),
            (b'%', 14),
            (0, 9),
            (127, 15),
        ] {
            state[Target::catcode(byte as usize)].value = cat;
        }
        state
    }
}
impl Scalars {
    pub fn counts(&self) -> &[Integer; 256] {
        self.0[..REGISTERS].try_into().unwrap()
    }
    pub fn catcodes(&self) -> &[Integer; 256] {
        self.0[CAT_BASE..ENDLINE].try_into().unwrap()
    }
}
impl Index<Target> for Scalars {
    type Output = Integer;
    #[inline]
    fn index(&self, target: Target) -> &Integer {
        &self.0[usize::from(target.0)]
    }
}
impl IndexMut<Target> for Scalars {
    #[inline]
    fn index_mut(&mut self, target: Target) -> &mut Integer {
        &mut self.0[usize::from(target.0)]
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PageParam {
    OutputPenalty = 23,
}
impl Target {
    pub fn page(parameter: PageParam) -> Self {
        Self::parameter(parameter as usize)
    }
}
