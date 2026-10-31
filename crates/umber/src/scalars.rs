use crate::token::Integer;
use std::ops::{Index, IndexMut};

// Scalar values have the same representation and group-save lifetime. A target
// identifies a slot directly, so assignment need not dispatch separately for
// reading, saving, and writing each kind of scalar table.
#[derive(Clone, Copy)]
pub(crate) struct Target(u16);
impl Target {
    pub const ENDLINE: Self = Self(512);
    pub const GLOBALDEFS: Self = Self(513);
    pub const ESCAPECHAR: Self = Self(514);
    pub fn count(index: usize) -> Self {
        Self(index as u16)
    }
    pub fn catcode(index: usize) -> Self {
        Self(256 + index as u16)
    }
    pub fn parameter(index: usize) -> Self {
        Self(513 + index as u16)
    }
    pub fn casecode(which: usize, index: usize) -> Self {
        Self(519 + (which * 256 + index) as u16)
    }
}

#[derive(Clone)]
pub(crate) struct Scalars([Integer; 1031]);
impl Default for Scalars {
    fn default() -> Self {
        let mut state = Self([Integer::default(); 1031]);
        state.0[256..512].fill(Integer {
            value: 12,
            level: 0,
        });
        state[Target::ENDLINE].value = 13;
        for (index, value) in [0, 92, 0, 1, 1, 1970].into_iter().enumerate() {
            state[Target::parameter(index)].value = value;
        }
        for byte in b'a'..=b'z' {
            for c in [byte, byte.to_ascii_uppercase()] {
                state[Target::catcode(c as usize)].value = 11;
                state[Target::casecode(0, c as usize)].value = i32::from(byte);
                state[Target::casecode(1, c as usize)].value = i32::from(byte.to_ascii_uppercase());
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
        self.0[..256].try_into().unwrap()
    }
    pub fn catcodes(&self) -> &[Integer; 256] {
        self.0[256..512].try_into().unwrap()
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
