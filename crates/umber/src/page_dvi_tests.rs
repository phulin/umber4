//! Minimal independent native-adapter DVI reader, not a document backend.
use crate::{Pages, Placement};
use std::collections::HashMap;
#[derive(Clone, Copy, Default)]
struct Position {
    h: i32,
    v: i32,
    w: i32,
    x: i32,
    y: i32,
    z: i32,
}
fn unsigned(bytes: &[u8], p: &mut usize, n: usize) -> u32 {
    let mut v = 0;
    for _ in 0..n {
        v = (v << 8) | u32::from(bytes[*p]);
        *p += 1;
    }
    v
}
fn signed(bytes: &[u8], p: &mut usize, n: usize) -> i32 {
    let v = unsigned(bytes, p, n);
    ((v << (32 - 8 * n)) as i32) >> (32 - 8 * n)
}
pub(crate) fn read(bytes: &[u8], pages: &Pages) -> Vec<Vec<Placement>> {
    let (mut p, mut pos, mut font) = (0, Position::default(), 0u32);
    let mut stack = Vec::new();
    let mut fonts = HashMap::new();
    let mut result: Vec<Vec<Placement>> = Vec::new();
    while p < bytes.len() {
        let op = bytes[p];
        p += 1;
        match op {
            0..=131 | 133..=136 => {
                let c = if op <= 127 {
                    u32::from(op)
                } else {
                    unsigned(
                        bytes,
                        &mut p,
                        (if op <= 131 { op - 127 } else { op - 132 }) as usize,
                    )
                };
                let (name, size) = &fonts[&font];
                let page = pages.get(result.len() - 1).unwrap();
                let local = page
                    .fonts()
                    .iter()
                    .position(|f| f.name() == name && f.size() == *size)
                    .unwrap();
                let c = u8::try_from(c).unwrap();
                result.last_mut().unwrap().push(Placement::Glyph {
                    font: local as u32,
                    character: c,
                    x: pos.h,
                    y: pos.v,
                });
                if op <= 131 {
                    pos.h += page.fonts()[local].metrics(c).unwrap().width;
                }
            }
            132 | 137 => {
                let h = signed(bytes, &mut p, 4);
                let w = signed(bytes, &mut p, 4);
                if h > 0 && w > 0 {
                    result.last_mut().unwrap().push(Placement::Rule {
                        x: pos.h,
                        y: pos.v - h,
                        width: w,
                        height: h,
                    });
                }
                if op == 132 {
                    pos.h += w;
                }
            }
            138 => {}
            139 => {
                p += 44;
                pos = Position::default();
                stack.clear();
                result.push(Vec::new());
            }
            140 => assert!(stack.is_empty()),
            141 => stack.push(pos),
            142 => pos = stack.pop().unwrap(),
            143..=146 => pos.h += signed(bytes, &mut p, (op - 142) as usize),
            147 => pos.h += pos.w,
            148..=151 => {
                pos.w = signed(bytes, &mut p, (op - 147) as usize);
                pos.h += pos.w;
            }
            152 => pos.h += pos.x,
            153..=156 => {
                pos.x = signed(bytes, &mut p, (op - 152) as usize);
                pos.h += pos.x;
            }
            157..=160 => pos.v += signed(bytes, &mut p, (op - 156) as usize),
            161 => pos.v += pos.y,
            162..=165 => {
                pos.y = signed(bytes, &mut p, (op - 161) as usize);
                pos.v += pos.y;
            }
            166 => pos.v += pos.z,
            167..=170 => {
                pos.z = signed(bytes, &mut p, (op - 166) as usize);
                pos.v += pos.z;
            }
            171..=234 => font = u32::from(op - 171),
            235..=238 => font = unsigned(bytes, &mut p, (op - 234) as usize),
            239..=242 => {
                let n = unsigned(bytes, &mut p, (op - 238) as usize);
                p += n as usize;
            }
            243..=246 => {
                let id = unsigned(bytes, &mut p, (op - 242) as usize);
                p += 4;
                let size = signed(bytes, &mut p, 4);
                p += 4;
                let area = unsigned(bytes, &mut p, 1) as usize;
                let name = unsigned(bytes, &mut p, 1) as usize;
                let value = bytes[p + area..p + area + name].to_vec();
                p += area + name;
                fonts.insert(id, (value, size));
            }
            247 => {
                assert_eq!(bytes[p], 2);
                p += 13;
                let n = unsigned(bytes, &mut p, 1);
                p += n as usize;
            }
            248 | 249 => break,
            _ => panic!("unexpected DVI opcode {op}"),
        }
    }
    result
}
