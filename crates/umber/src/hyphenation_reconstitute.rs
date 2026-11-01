//! TeX's cut-prefix reconstruction over bounded original character codes.
use crate::nodes::KernKind;
use crate::{
    Result,
    nodes::{DetachedNode, Kind, List, NodeId, NodeStore, ReleaseScratch},
    tfm::{Resource, ScaledFont},
};

#[derive(Clone, Copy)]
struct Item {
    character: u16,
    original: Option<u8>,
}
#[derive(Default)]
pub(super) struct Scratch {
    stack: Vec<Item>,
    release: ReleaseScratch,
    #[cfg(test)]
    fail_after: Option<usize>,
}
#[cfg(test)]
impl Scratch {
    pub fn fail_after(&mut self, allocations: Option<usize>) {
        self.fail_after = allocations;
    }
    pub fn capacity(&self) -> usize {
        self.stack.capacity()
    }
}
#[derive(Clone, Copy)]
pub(super) struct Initial {
    pub components: NodeId,
    pub ligature: bool,
    pub left: bool,
}
impl Default for Initial {
    fn default() -> Self {
        Self {
            components: NodeId::NULL,
            ligature: false,
            left: false,
        }
    }
}
pub(super) struct Reconstituter<'a> {
    pub nodes: &'a mut NodeStore,
    pub font: u32,
    pub metrics: &'a ScaledFont,
    pub resource: &'a Resource,
    pub original: &'a mut [u16; 64],
    pub initial: Initial,
    pub scratch: &'a mut Scratch,
}
struct Cursor {
    j: usize,
    left: u16,
    right: u16,
    hypothetical: u16,
    hyphen: u16,
    boundary: u16,
    ligature: bool,
    left_hit: bool,
    right_hit: bool,
    passed: usize,
}
impl Reconstituter<'_> {
    /// Append a cut-prefix translation and return its final input index and
    /// first affected hyphen. All appended nodes belong to output immediately.
    pub fn run(
        &mut self,
        j: usize,
        n: usize,
        boundary: u16,
        hyphen: u16,
        positions: u64,
        output: &mut List,
    ) -> Result<(usize, usize)> {
        debug_assert!(self.scratch.stack.is_empty());
        let mut components = List::default();
        let mut c = Cursor {
            j,
            left: self.original[j],
            right: 256,
            hypothetical: 256,
            hyphen,
            boundary,
            ligature: false,
            left_hit: false,
            right_hit: false,
            passed: 0,
        };
        let result = (|| {
            if j == 0 {
                c.ligature = self.initial.ligature;
                c.left_hit = c.ligature && self.initial.left;
                let mut p = self.initial.components;
                while !p.is_null() {
                    let node = self.nodes.get(p);
                    let Kind::Character { character, .. } = node.kind else {
                        unreachable!("ligature components are characters")
                    };
                    // A preceding ordinary character's successor is the word,
                    // whereas a ligature's components have their own chain.
                    let next = if self.initial.ligature {
                        node.next
                    } else {
                        NodeId::NULL
                    };
                    self.character(&mut components, character)?;
                    p = next;
                }
            } else if c.left < 256 {
                self.character(&mut components, c.left as u8)?;
            }
            self.set_right(&mut c, n, positions);
            loop {
                let test = if c.hypothetical < 256 {
                    c.hypothetical
                } else {
                    c.right
                };
                let instruction = if c.left == 256 {
                    self.resource
                        .boundary_label
                        .and_then(|start| self.resource.lig_instruction(c.left, test, Some(start)))
                } else {
                    self.resource.lig_instruction(c.left, test, None)
                };
                // The hypothetical hyphen must be searched independently of
                // the actual right character; their instruction order differs.
                if c.hypothetical < 256 {
                    if instruction.is_some() {
                        c.passed = c.j;
                        c.hyphen = 256;
                    }
                    c.hypothetical = 256;
                    continue;
                }
                let mut kern = None;
                let mut finish = true;
                if let Some([_, _, op, rem]) = instruction {
                    if c.hyphen < 256 && positions & (1u64 << c.j) != 0 {
                        c.passed = c.j;
                        c.hyphen = 256;
                    }
                    if op >= 128 {
                        kern = Some(
                            self.metrics
                                .kern(256 * usize::from(op - 128) + usize::from(rem)),
                        );
                    } else {
                        if c.left == 256 {
                            c.left_hit = true;
                        }
                        if c.j == n && self.scratch.stack.is_empty() {
                            c.right_hit = true;
                        }
                        match op {
                            1 | 5 => {
                                c.left = rem.into();
                                c.ligature = true;
                            }
                            2 | 6 => {
                                c.right = rem.into();
                                if let Some(item) = self.scratch.stack.last_mut() {
                                    item.character = c.right;
                                } else {
                                    let original = if c.j == n {
                                        c.boundary = 256;
                                        None
                                    } else {
                                        Some(self.original[c.j + 1] as u8)
                                    };
                                    self.scratch.stack.push(Item {
                                        character: c.right,
                                        original,
                                    });
                                }
                            }
                            3 => {
                                c.right = rem.into();
                                self.scratch.stack.push(Item {
                                    character: c.right,
                                    original: None,
                                });
                            }
                            7 | 11 => {
                                self.wrap(&mut c, &mut components, output, false)?;
                                c.left = rem.into();
                                c.ligature = true;
                            }
                            _ => {
                                c.left = rem.into();
                                c.ligature = true;
                                if !self.scratch.stack.is_empty() {
                                    self.pop(&mut c, n, positions, &mut components)?;
                                } else if c.j < n {
                                    self.character(&mut components, c.right as u8)?;
                                    c.j += 1;
                                    self.set_right(&mut c, n, positions);
                                } else {
                                    self.wrap(&mut c, &mut components, output, true)?;
                                    return Ok((c.j, c.passed));
                                }
                            }
                        }
                        finish = op > 4 && op != 7;
                    }
                }
                if !finish {
                    continue;
                }
                self.wrap(&mut c, &mut components, output, true)?;
                if let Some(width) = kern.filter(|w| w.0 != 0) {
                    let node = self.alloc(Kind::Kern {
                        width,
                        kind: KernKind::Normal,
                    })?;
                    self.nodes.append(output, node);
                }
                if self.scratch.stack.is_empty() {
                    return Ok((c.j, c.passed));
                }
                c.left = self.scratch.stack.last().unwrap().character;
                c.ligature = true;
                self.pop(&mut c, n, positions, &mut components)?;
            }
        })();
        self.nodes.release(components, &mut self.scratch.release);
        self.scratch.stack.clear();
        result
    }
    fn character(&mut self, output: &mut List, character: u8) -> Result<()> {
        let node = self.alloc(Kind::Character {
            font: self.font,
            character,
        })?;
        self.nodes.append(output, node);
        Ok(())
    }
    fn set_right(&self, c: &mut Cursor, n: usize, positions: u64) {
        c.right = if c.j < n {
            self.original[c.j + 1]
        } else {
            c.boundary
        };
        c.hypothetical = if positions & (1u64 << c.j) != 0 {
            c.hyphen
        } else {
            256
        };
    }
    fn pop(
        &mut self,
        c: &mut Cursor,
        n: usize,
        positions: u64,
        components: &mut List,
    ) -> Result<()> {
        let item = self.scratch.stack.pop().unwrap();
        if let Some(character) = item.original {
            self.character(components, character)?;
            c.j += 1;
        }
        if let Some(item) = self.scratch.stack.last() {
            c.right = item.character;
        } else {
            self.set_right(c, n, positions);
        }
        Ok(())
    }
    fn wrap(
        &mut self,
        c: &mut Cursor,
        components: &mut List,
        output: &mut List,
        boundary: bool,
    ) -> Result<()> {
        if c.ligature {
            let right = boundary && c.right_hit && self.scratch.stack.is_empty();
            let node = self.alloc(Kind::ligature(
                self.font,
                c.left as u8,
                std::mem::take(components),
                c.left_hit,
                right,
            ))?;
            self.nodes.append(output, node);
            c.ligature = false;
            c.left_hit = false;
            if right {
                c.right_hit = false;
            }
        } else {
            self.nodes.splice(output, std::mem::take(components));
        }
        Ok(())
    }
    pub fn alloc(&mut self, kind: Kind) -> Result<DetachedNode> {
        let node = self.nodes.alloc(kind)?;
        #[cfg(test)]
        if let Some(remaining) = &mut self.scratch.fail_after {
            if *remaining == 0 {
                self.nodes.release_node(node, &mut self.scratch.release);
                return Err(crate::Error::Capacity);
            }
            *remaining -= 1;
        }
        Ok(node)
    }
}
