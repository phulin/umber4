//! TeX's ligature cursor: original components live in node lists, not glyph heaps.
use crate::{
    Engine, Result,
    nodes::{DetachedNode, Kind, List, ReleaseScratch},
    token::*,
};
struct Item {
    character: u16,
    original: Option<DetachedNode>,
    pseudo: bool,
}
#[derive(Default)]
pub(crate) struct Scratch {
    stack: Vec<Item>,
    release: ReleaseScratch,
}
#[derive(Clone, Copy)]
enum State {
    Move,
    Consume,
    Look,
    Program,
    Wrap,
}
impl Engine<'_> {
    pub(crate) fn character_run(&mut self, first: u8) -> Result<()> {
        let mut scratch = std::mem::take(&mut self.ligatures);
        let mut components = List::default();
        let result = self.character_run_inner(first, &mut scratch, &mut components);
        self.document
            .nodes
            .release(components, &mut scratch.release);
        for item in scratch.stack.drain(..) {
            if let Some(node) = item.original {
                self.document.nodes.release_node(node, &mut scratch.release);
            }
        }
        self.ligatures = scratch;
        result
    }
    fn character_run_inner(
        &mut self,
        first: u8,
        scratch: &mut Scratch,
        components: &mut List,
    ) -> Result<()> {
        let font = self.document.fonts.current.value as u32;
        self.adjust_space_factor(first);
        let Some(scaled) = self.document.fonts.descriptors[font as usize].scaled else {
            // Missing nullfont glyphs are discarded, but their sfcode still ran.
            self.document.text.cancel_boundary = false;
            return Ok(());
        };
        let resource = self.font_catalog.scaled[scaled].descriptor().resource;
        let r = &self.font_catalog.resources[resource];
        let mut boundary = r.boundary;
        let false_boundary = r.false_boundary;
        let boundary_label = r.boundary_label;
        let mut start = if std::mem::take(&mut self.document.text.cancel_boundary) {
            None
        } else {
            boundary_label
        };
        let node = self.document.nodes.alloc(Kind::Character {
            font,
            character: first,
        })?;
        scratch.stack.push(Item {
            character: first as u16,
            original: Some(node),
            pseudo: false,
        });
        let mut left = 256u16;
        let mut right = first as u16;
        let mut ligature = false;
        let mut left_hit = false;
        let mut right_hit = false;
        let mut pending = None;
        let mut state = if start.is_some() {
            State::Program
        } else {
            State::Move
        };
        loop {
            match state {
                State::Move => {
                    let Some(item) = scratch.stack.last() else {
                        if let Some(read) = pending {
                            self.back_read(read);
                        }
                        return Ok(());
                    };
                    left = item.character;
                    state = State::Consume;
                }
                State::Consume => {
                    let item = scratch.stack.pop().unwrap();
                    if !item.pseudo
                        && !self.font_catalog.scaled[scaled]
                            .descriptor()
                            .exists(left as u8)
                    {
                        if let Some(node) = item.original {
                            self.document.nodes.release_node(node, &mut scratch.release);
                        }
                        if let Some(read) = pending {
                            self.back_read(read);
                        }
                        return Ok(());
                    }
                    let original = item.original.is_some();
                    if let Some(node) = item.original {
                        self.document.nodes.append(components, node);
                    }
                    if item.pseudo {
                        ligature = true;
                        if let Some(next) = scratch.stack.last() {
                            right = next.character;
                            state = State::Program;
                        } else if original {
                            state = State::Look;
                        } else {
                            right = boundary;
                            state = State::Program;
                        }
                    } else {
                        state = State::Look;
                    }
                }
                State::Look => {
                    let literal = if self.document.sources.is_empty() {
                        self.document
                            .file
                            .read_character(self.document.scalars.catcodes())
                    } else {
                        None
                    };
                    let c = if let Some(token) = literal {
                        token.byte()
                    } else {
                        let mut read = self.next_token()?;
                        while read.meaning.expandable() {
                            if read.meaning.is_macro() {
                                self.begin_macro(read.meaning.value as u32)?;
                            } else {
                                self.expand(read)?;
                            }
                            read = self.next_token()?;
                        }
                        match read.meaning.command {
                            11 | 12 | cmd::CHAR_GIVEN => read.meaning.value as u8,
                            cmd::CHAR => self.scan_index()? as u8,
                            _ => {
                                if read.meaning.command == cmd::NO_BOUNDARY {
                                    boundary = 256;
                                }
                                right = boundary;
                                pending = Some(read);
                                state = State::Program;
                                continue;
                            }
                        }
                    };
                    self.adjust_space_factor(c);
                    let node = self
                        .document
                        .nodes
                        .alloc(Kind::Character { font, character: c })?;
                    scratch.stack.push(Item {
                        character: c as u16,
                        original: Some(node),
                        pseudo: false,
                    });
                    right = if c as u16 == false_boundary {
                        256
                    } else {
                        c as u16
                    };
                    state = State::Program;
                }
                State::Wrap => {
                    self.wrap_ligature(
                        font,
                        left,
                        components,
                        &mut ligature,
                        &mut left_hit,
                        &mut right_hit,
                        scratch.stack.is_empty(),
                    )?;
                    state = State::Move;
                }
                State::Program => {
                    let instruction = self.lig_instruction(resource, left, right, start.take());
                    let Some([_, _, op, rem]) = instruction else {
                        state = State::Wrap;
                        continue;
                    };
                    if op >= 128 {
                        self.wrap_ligature(
                            font,
                            left,
                            components,
                            &mut ligature,
                            &mut left_hit,
                            &mut right_hit,
                            scratch.stack.is_empty(),
                        )?;
                        let width = self
                            .font_catalog
                            .kern(scaled, 256 * (op as usize - 128) + rem as usize);
                        self.append_horizontal(Kind::Kern {
                            width,
                            explicit: false,
                        })?;
                        state = State::Move;
                        continue;
                    }
                    if left == 256 {
                        left_hit = true;
                    } else if scratch.stack.is_empty() {
                        right_hit = true;
                    }
                    match op {
                        1 | 5 => {
                            left = rem as u16;
                            ligature = true;
                        }
                        2 | 6 => {
                            right = rem as u16;
                            if let Some(item) = scratch.stack.last_mut() {
                                item.character = right;
                                item.pseudo = true;
                            } else {
                                scratch.stack.push(Item {
                                    character: right,
                                    original: None,
                                    pseudo: true,
                                });
                                boundary = 256;
                            }
                        }
                        3 => {
                            right = rem as u16;
                            scratch.stack.push(Item {
                                character: right,
                                original: None,
                                pseudo: true,
                            });
                        }
                        7 | 11 => {
                            self.wrap_ligature(
                                font,
                                left,
                                components,
                                &mut ligature,
                                &mut left_hit,
                                &mut right_hit,
                                false,
                            )?;
                            left = rem as u16;
                            ligature = true;
                        }
                        _ => {
                            left = rem as u16;
                            ligature = true;
                            state = if scratch.stack.is_empty() {
                                State::Wrap
                            } else {
                                State::Consume
                            };
                            continue;
                        }
                    }
                    if op > 4 && op != 7 {
                        state = State::Wrap;
                    } else {
                        if left == 256 {
                            start = boundary_label;
                        }
                        state = State::Program;
                    }
                }
            }
        }
    }
    fn lig_instruction(
        &self,
        resource: usize,
        left: u16,
        right: u16,
        start: Option<usize>,
    ) -> Option<[u8; 4]> {
        if right == 256 {
            return None;
        }
        let r = &self.font_catalog.resources[resource];
        let mut i = if let Some(i) = start {
            i
        } else {
            if left == 256 || (left as usize) < r.bc || left as usize > r.ec {
                return None;
            }
            let info = r.chars[left as usize - r.bc];
            if info.tag != 1 {
                return None;
            }
            let mut i = info.remainder;
            let p = r.program[i];
            if p[0] > 128 {
                i = 256 * p[2] as usize + p[3] as usize;
            }
            i
        };
        loop {
            let p = r.program[i];
            if p[0] <= 128 && p[1] as u16 == right {
                return Some(p);
            }
            if p[0] >= 128 {
                return None;
            }
            i += p[0] as usize + 1;
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn wrap_ligature(
        &mut self,
        font: u32,
        left: u16,
        components: &mut List,
        ligature: &mut bool,
        left_hit: &mut bool,
        right_hit: &mut bool,
        at_boundary: bool,
    ) -> Result<()> {
        if left == 256 {
            return Ok(());
        }
        let hyphen = self.document.fonts.descriptors[font as usize].hyphen;
        let disc = if components.is_empty() {
            false
        } else if !*ligature {
            // An unchanged left character owns exactly its original component.
            i32::from(left) == hyphen
        } else {
            matches!(self.document.nodes.get(components.tail()).kind,Kind::Character{character,..} if i32::from(character)==hyphen)
        };
        if *ligature {
            self.append_horizontal(Kind::ligature(
                font,
                left as u8,
                std::mem::take(components),
                *left_hit,
                *right_hit && at_boundary,
            ))?;
            *ligature = false;
            *left_hit = false;
            if at_boundary {
                *right_hit = false;
            }
        } else {
            self.document.nodes.splice(
                &mut self.document.text.horizontal,
                std::mem::take(components),
            );
        }
        if disc {
            self.append_horizontal(Kind::discretionary(List::default(), List::default(), 0))?;
        }
        Ok(())
    }
}
