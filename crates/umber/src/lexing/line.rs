use super::INVALID;
use super::hash::NameHash;
use super::scan::Out;
use crate::names::Names;
use crate::token::{NULL_CS, Token};

// The current reader's character steps on one prepared line, which ends in
// the end-of-line character when there is one. A fast-path stop resumes here
// from its token's position and lexical state; `^^` translation rewrites the
// line exactly as the reader does.

fn classified(line: &mut Vec<u8>, at: usize, cats: &[u8; 256]) -> (u8, u8) {
    let byte = line[at];
    if cats[byte as usize] != 7 {
        return (byte, cats[byte as usize]);
    }
    let byte = translated(line, at, cats);
    (byte, cats[byte as usize])
}

fn translated(line: &mut Vec<u8>, at: usize, cats: &[u8; 256]) -> u8 {
    let hex = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    };
    loop {
        let b = line[at];
        if cats[b as usize] != 7 || line.get(at + 1) != Some(&b) || at + 2 >= line.len() {
            return b;
        }
        let c = line[at + 2];
        let (decoded, end) = match (hex(c), line.get(at + 3).copied().and_then(hex)) {
            (Some(high), Some(low)) => (high * 16 + low, at + 4),
            _ if c < 128 => (c ^ 64, at + 3),
            _ => return b,
        };
        line[at] = decoded;
        line.drain(at + 1..end);
    }
}

pub(super) fn lex<H: NameHash>(
    line: &mut Vec<u8>,
    mut position: usize,
    mut state: u8,
    cats: &[u8; 256],
    names: &mut Names,
    par: u32,
    out: &mut Out,
) {
    while position < line.len() {
        let (byte, cat) = classified(line, position, cats);
        position += 1;
        match cat {
            0 => {
                state = 2;
                if position == line.len() {
                    out.push(Token::cs(NULL_CS).0);
                    continue;
                }
                let (first, cat) = classified(line, position, cats);
                if cat != 11 {
                    position += 1;
                    state = if cat == 10 { 2 } else { 1 };
                    out.push(Token::single(first).0);
                    continue;
                }
                let start = position;
                position += 1;
                loop {
                    while position < line.len() && cats[line[position] as usize] == 11 {
                        position += 1;
                    }
                    if position == line.len() || classified(line, position, cats).1 != 11 {
                        break;
                    }
                    position += 1;
                }
                let name = &line[start..position];
                out.push(if name.len() == 1 {
                    Token::single(first).0
                } else {
                    Token::cs(names.intern_hashed(name, H::exact(name)).unwrap()).0
                });
            }
            5 => {
                position = line.len();
                match std::mem::replace(&mut state, 0) {
                    0 => out.push(par),
                    1 => out.push(Token::SPACE.0),
                    _ => {}
                }
            }
            9 => {}
            10 => {
                if std::mem::replace(&mut state, 2) == 1 {
                    out.push(Token::SPACE.0);
                }
            }
            13 => {
                state = 1;
                out.push(Token::active(byte).0);
            }
            14 => position = line.len(),
            15 => out.push(INVALID),
            _ => {
                state = 1;
                out.push(Token::character(cat, byte).0);
            }
        }
    }
}
