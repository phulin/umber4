//! Primitive vertical contributions and page-visible quantities.
use crate::{
    Engine, Error, Result,
    nodes::{GlueKind, Kind},
    numeric::{Glue, GlueOrder, Scaled},
    page_output::RUNNING,
    quantities::Quantity,
    text::Mode,
    token::*,
};
impl Engine<'_> {
    pub(crate) fn page_assignment(&mut self, read: Read) -> Result<()> {
        if read.meaning.command == cmd::PREV_DEPTH && !self.document.text.mode.is_vertical() {
            return Err(Error::Syntax);
        }
        self.scan_equals()?;
        let value = self.scan_dimen()?;
        if read.meaning.command == cmd::PAGE_DIMEN {
            self.document.page.state.totals[read.meaning.value as usize] = value.0;
        } else {
            self.document.text.prev_depth = value;
        }
        Ok(())
    }
    pub(crate) fn page_quantity(&self, read: Read) -> Result<Option<Quantity>> {
        Ok(match read.meaning.command {
            cmd::PAGE_DIMEN => {
                let index = read.meaning.value as usize;
                Some(Quantity::Dimension(
                    if self.document.output_frame.is_some() {
                        // start_new_page retains raw totals for output_text reads.
                        Scaled(self.document.page.state.totals[index])
                    } else {
                        self.document.page.quantity(index)
                    },
                ))
            }
            cmd::PREV_DEPTH => {
                if !self.document.text.mode.is_vertical() {
                    return Err(Error::Syntax);
                }
                Some(Quantity::Dimension(self.document.text.prev_depth))
            }
            cmd::LAST_ITEM => {
                if self.document.text.mode.is_math() {
                    return Err(Error::Unsupported);
                }
                if self.document.text.mode == Mode::Write {
                    return Ok(Some(match read.meaning.value {
                        0 => Quantity::Integer(0),
                        1 => Quantity::Dimension(Scaled::ZERO),
                        _ => Quantity::Glue(crate::numeric::Glue::default()),
                    }));
                }
                let list = if self.document.text.mode.is_horizontal() {
                    &self.document.text.horizontal
                } else {
                    &self.document.text.vertical
                };
                let kind = if list.is_empty() {
                    None
                } else {
                    Some(&self.document.nodes.get(list.tail()).kind)
                };
                let state = &self.document.page.state;
                let vertical_empty = list.is_empty() && self.document.text.mode == Mode::Vertical;
                Some(match read.meaning.value {
                    0 => Quantity::Integer(match kind {
                        Some(Kind::Penalty(p)) => *p,
                        _ if vertical_empty => state.last_penalty,
                        _ => 0,
                    }),
                    1 => Quantity::Dimension(Scaled(match kind {
                        Some(Kind::Kern { width, .. }) => width.0,
                        _ if vertical_empty => state.last_kern,
                        _ => 0,
                    })),
                    _ => Quantity::Glue(match kind {
                        Some(kind) if kind.is_glue() => self.document.nodes.glue(kind).unwrap().0,
                        _ if vertical_empty => state.last_glue.unwrap_or_default(),
                        _ => Glue::default(),
                    }),
                })
            }
            _ => None,
        })
    }
    pub(crate) fn vertical_command(&mut self, read: Read) -> Result<bool> {
        match read.meaning.command {
            cmd::LEADERS => self.begin_leaders(read.meaning.value)?,
            cmd::INSERT_ADJUST => {
                self.begin_insertion(read.meaning.value != 0)?;
            }
            cmd::UNSUPPORTED_LAYOUT => return Err(Error::Unsupported),
            cmd::VSKIP => {
                if self.document.text.mode == Mode::RestrictedHorizontal {
                    return Err(Error::Syntax);
                }
                if self.document.text.mode == Mode::Horizontal {
                    self.head_for_vertical(read)?;
                    return Ok(true);
                }
                let spec = self.scan_skip(read.meaning.value)?;
                let node = self.document.nodes.alloc(Kind::Glue {
                    spec,
                    subtype: GlueKind::Normal,
                })?;
                self.document
                    .nodes
                    .append(&mut self.document.text.vertical, node);
            }
            cmd::RULE => {
                let horizontal = read.meaning.value == 1;
                if !horizontal && self.document.text.mode == Mode::RestrictedHorizontal {
                    return Err(Error::Syntax);
                }
                if !horizontal && self.document.text.mode == Mode::Horizontal {
                    self.head_for_vertical(read)?;
                    return Ok(true);
                }
                if horizontal && self.document.text.mode.is_vertical() {
                    self.back_read(read)?;
                    self.new_graf(true)?;
                    return Ok(true);
                }
                let rule = self.scan_rule(horizontal)?;
                let node = self.document.nodes.alloc(rule)?;
                if horizontal {
                    self.document
                        .nodes
                        .append(&mut self.document.text.horizontal, node);
                    self.document.text.space_factor = 1000;
                } else {
                    self.document
                        .nodes
                        .append(&mut self.document.text.vertical, node);
                    self.document.text.prev_depth = Scaled(-65_536_000);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    pub(crate) fn scan_rule(&mut self, horizontal: bool) -> Result<Kind> {
        let (mut width, mut height, mut depth) = if horizontal {
            (Scaled(26214), Scaled(RUNNING), Scaled(RUNNING))
        } else {
            (Scaled(RUNNING), Scaled(26214), Scaled::ZERO)
        };
        loop {
            if self.scan_keyword(b"width")? {
                width = self.scan_dimen()?;
            } else if self.scan_keyword(b"height")? {
                height = self.scan_dimen()?;
            } else if self.scan_keyword(b"depth")? {
                depth = self.scan_dimen()?;
            } else {
                return Ok(Kind::Rule {
                    width,
                    height,
                    depth,
                });
            }
        }
    }
    pub(crate) fn scan_skip(&mut self, operation: i32) -> Result<Glue> {
        Ok(match operation {
            0 => self.scan_glue()?,
            n => Glue {
                stretch: Scaled(if n == 4 { -65536 } else { 65536 }),
                stretch_order: if n == 2 {
                    GlueOrder::Fill
                } else {
                    GlueOrder::Fil
                },
                shrink: Scaled(if n == 3 { 65536 } else { 0 }),
                shrink_order: if n == 3 {
                    GlueOrder::Fil
                } else {
                    GlueOrder::Normal
                },
                ..Glue::default()
            },
        })
    }
}
