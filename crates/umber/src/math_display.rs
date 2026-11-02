//! Ordinary display entry, math-shift closure, and horizontal continuation.
use super::*;
use crate::{
    language::Language,
    nodes::{GlueSign, List},
    scalars::ParagraphParam,
};

impl Engine<'_> {
    pub(super) fn enter_display_math(&mut self) -> Result<()> {
        let widow = self.document.scalars[Target::DISPLAY_WIDOW].value;
        let last = self.finish_paragraph(widow)?;
        let pre = last.map_or(-Scaled::MAX.0, |line| self.pre_display_size(line));
        let (width, indent) = self.display_shape();
        self.open_group(GroupKind::MathShift)?;
        let enclosing_mode = self.document.text.mode;
        self.document.text.mode = Mode::DisplayMath;
        // Unlike inline math, retain the ordinary vertical root here so page
        // construction/output can use it while this math frame remains active.
        self.document.math_frames.push(Frame {
            list: MathList::default(),
            incomplete: None,
            context: Context::Display {
                number: None,
                left: false,
            },
            level: self.document.level,
            enclosing_mode,
        });
        self.update_integer(Target::FAM, false, |_| Ok(-1))?;
        for (parameter, value) in [
            (DimenParam::PreDisplaySize, pre),
            (DimenParam::DisplayWidth, width),
            (DimenParam::DisplayIndent, indent),
        ] {
            self.assign_dimen(parameter.index(), Scaled(value), false);
        }
        if let Some(owner) = self.document.token_registers[261].owner.clone() {
            self.push_tokens(owner, 0)?;
        }
        if enclosing_mode == Mode::Vertical {
            self.build_page()?;
        }
        Ok(())
    }
    fn pre_display_size(&self, line: nodes::NodeId) -> i32 {
        let nodes::Kind::Box(id) = &self.document.nodes.get(line).kind else {
            unreachable!()
        };
        let b = self.document.nodes.box_data(id);
        let mut v = b
            .shift
            .0
            .wrapping_add(self.font_unit(false).0.wrapping_mul(2));
        let mut w = -Scaled::MAX.0;
        let mut p = b.list.head();
        while !p.is_null() {
            let n = self.document.nodes.get(p);
            let (d, visible) = match &n.kind {
                nodes::Kind::Character { font, character }
                | nodes::Kind::Ligature {
                    font, character, ..
                } => (
                    self.character_metrics(*font, *character)
                        .expect("line glyph")
                        .width
                        .0,
                    true,
                ),
                nodes::Kind::Box(id) => (self.document.nodes.box_data(id).width.0, true),
                nodes::Kind::Rule { width, .. } => (width.0, true),
                nodes::Kind::Kern { width, .. } | nodes::Kind::MathBoundary { width, .. } => {
                    (width.0, false)
                }
                kind if kind.is_glue() => {
                    let (glue, subtype) = self.document.nodes.glue(kind).unwrap();
                    if (b.glue_set.sign == GlueSign::Stretching
                        && b.glue_set.order == glue.stretch_order
                        && glue.stretch.0 != 0)
                        || (b.glue_set.sign == GlueSign::Shrinking
                            && b.glue_set.order == glue.shrink_order
                            && glue.shrink.0 != 0)
                    {
                        v = Scaled::MAX.0;
                    }
                    (glue.width.0, subtype.is_leader())
                }
                _ => (0, false),
            };
            if visible {
                if v >= Scaled::MAX.0 {
                    return Scaled::MAX.0;
                }
                v = v.wrapping_add(d);
                w = v;
            } else if v < Scaled::MAX.0 {
                v = v.wrapping_add(d);
            }
            p = n.next;
        }
        w
    }
    fn display_shape(&self) -> (i32, i32) {
        let d = &self.document;
        let line = d.text.prev_graf.wrapping_add(2);
        if let Some(shape) = &d.layout_values.shape.lines {
            let entry = shape[(line as usize).min(shape.len()) - 1];
            return (entry.width.0, entry.indent.0);
        }
        let hsize = d.layout_values.dimens[DimenParam::HSize.index()].value.0;
        let hang = d.layout_values.dimens[DimenParam::HangIndent.index()]
            .value
            .0;
        let after = d.scalars[Target::paragraph(ParagraphParam::HangAfter)].value;
        if hang != 0
            && ((after >= 0 && line > after)
                || (after < 0 && d.text.prev_graf.wrapping_add(1) < after.wrapping_neg()))
        {
            (hsize.wrapping_sub(hang.wrapping_abs()), hang.max(0))
        } else {
            (hsize, 0)
        }
    }
    pub(super) fn begin_equation_number(&mut self, left: bool) -> Result<()> {
        if self.document.text.mode != Mode::DisplayMath {
            return Err(Error::Syntax);
        }
        if self.current_group() != Some(GroupKind::MathShift) {
            return Err(Error::Group);
        }
        self.open_group(GroupKind::MathShift)?;
        self.document.math_frames.push(Frame {
            list: MathList::default(),
            incomplete: None,
            context: Context::EquationNumber { left },
            level: self.document.level,
            enclosing_mode: Mode::DisplayMath,
        });
        self.document.text.mode = Mode::Math;
        self.update_integer(Target::FAM, false, |_| Ok(-1))?;
        if let Some(owner) = self.document.token_registers[260].owner.clone() {
            self.push_tokens(owner, 0)?;
        }
        Ok(())
    }
    pub(super) fn finish_math_shift(&mut self) -> Result<()> {
        if self.current_group() != Some(GroupKind::MathShift) {
            return Err(Error::Group);
        }
        match self.document.math_frames.last().unwrap().context {
            Context::Display { .. } => self.close_display(true),
            Context::EquationNumber { .. } => self.close_equation_number(),
            Context::Inline(_) => self.finish_inline_math(),
            _ => unreachable!("math shift group"),
        }
    }
    fn closing_display_shift(&mut self, mode: Mode) -> Result<()> {
        // Native fin_mlist has popped the nest before get_x_token. Retain the
        // affine frame until the closer is validated, but expose that mode.
        let old = std::mem::replace(&mut self.document.text.mode, mode);
        let result = self.next_expanded().and_then(|read| {
            if read.meaning.command == 3 {
                Ok(())
            } else {
                self.back(read.token)?;
                Err(Error::Syntax)
            }
        });
        if result.is_err() {
            self.document.text.mode = old;
        }
        result
    }
    fn close_equation_number(&mut self) -> Result<()> {
        self.validate_math_fonts()?;
        self.closing_display_shift(Mode::DisplayMath)?;
        let frame = self.document.math_frames.pop().unwrap();
        let Context::EquationNumber { left } = frame.context else {
            unreachable!()
        };
        let list = self
            .math
            .as_mut()
            .unwrap()
            .finish_list(frame.list, frame.incomplete, None);
        let list = self.convert_math(list, Style::TEXT, false)?;
        let number = self.hpack(list, crate::packing::PackSpec::Natural)?;
        let Context::Display {
            number: slot,
            left: side,
        } = &mut self.document.math_frames.last_mut().unwrap().context
        else {
            unreachable!()
        };
        *slot = Some(number);
        *side = left;
        self.close_group(GroupKind::MathShift)?;
        self.close_display(false)
    }
    fn close_display(&mut self, scan_shift: bool) -> Result<()> {
        self.validate_math_fonts()?;
        if scan_shift {
            self.closing_display_shift(self.document.text.vertical_mode)?;
        }
        self.document.text.mode = self.document.text.vertical_mode;
        let frame = self.document.math_frames.pop().unwrap();
        let Context::Display { number, left } = frame.context else {
            unreachable!()
        };
        let list = self
            .math
            .as_mut()
            .unwrap()
            .finish_list(frame.list, frame.incomplete, None);
        let mut work = layout::Work {
            formula: None,
            number,
            adjustments: List::default(),
        };
        let result = (|| {
            let list = self.convert_math(list, Style::DISPLAY, false)?;
            let (node, totals) = self.hpack_adjusted_with_totals(
                list,
                crate::packing::PackSpec::Natural,
                &mut work.adjustments,
            )?;
            work.formula = Some(node);
            self.layout_display(&mut work, totals, left)?;
            self.close_group(GroupKind::MathShift)?;
            self.resume_display()
        })();
        work.release(self);
        result
    }
    pub(crate) fn finish_display_alignment(
        &mut self,
        mut body: List,
        depth: Scaled,
        trailing_rule: Option<nodes::NodeId>,
    ) -> Result<()> {
        let result = (|| {
            let first = self.do_assignments()?;
            if first.meaning.command != 3 {
                self.back(first.token)?;
                return Err(Error::Syntax);
            }
            let second = self.next_expanded()?;
            if second.meaning.command != 3 {
                self.back(second.token)?;
                return Err(Error::Syntax);
            }
            let frame = self.document.math_frames.last().ok_or(Error::Group)?;
            if self.current_group() != Some(GroupKind::MathShift)
                || self.document.level != frame.level
                || !matches!(frame.context, Context::Display { number: None, .. })
                || !frame.list.is_empty()
                || frame.incomplete.is_some()
            {
                return Err(Error::Group);
            }
            self.document.math_frames.pop();
            self.document.text.mode = self.document.text.vertical_mode;
            self.display_node(nodes::Kind::Penalty(
                self.document.scalars[Target::PRE_DISPLAY].value,
            ))?;
            self.display_skip(crate::layout_values::GlueParam::AboveDisplay)?;
            self.document
                .nodes
                .splice(&mut self.document.text.vertical, std::mem::take(&mut body));
            let below = self.document.layout_values.glues
                [crate::layout_values::GlueParam::BelowDisplay.index()]
            .value;
            for kind in [
                nodes::Kind::Penalty(self.document.scalars[Target::POST_DISPLAY].value),
                nodes::Kind::Glue {
                    spec: below,
                    subtype: nodes::GlueKind::Normal,
                },
            ] {
                let node = self.document.nodes.alloc(kind)?;
                if let Some(wrapper) = trailing_rule {
                    // fin_align keeps its old tail at the rule child after
                    // wrapping. Preserve these post-pack appends until external
                    // page work resets the contribution append position.
                    let mut child = std::mem::take(&mut self.document.nodes.box_mut(wrapper).list);
                    self.document.nodes.append(&mut child, node);
                    self.document.nodes.box_mut(wrapper).list = child;
                } else {
                    self.document
                        .nodes
                        .append(&mut self.document.text.vertical, node);
                }
            }
            self.document.text.prev_depth = depth;
            self.close_group(GroupKind::MathShift)?;
            self.resume_display()
        })();
        self.document.nodes.release(body, &mut self.page_release);
        result
    }
    fn resume_display(&mut self) -> Result<()> {
        self.document.text.prev_graf = self.document.text.prev_graf.wrapping_add(3);
        self.document.text.mode = Mode::Horizontal;
        self.document.text.space_factor = 1000;
        self.document.text.cancel_boundary = false;
        let language = Language::capture(
            &self.document.scalars,
            self.document.scalars[Target::LANGUAGE].value,
        );
        self.document.text.language = language.number;
        self.document.text.initial_language = language;
        let read = self.next_expanded()?;
        if read.meaning.command != cmd::SPACE {
            self.back(read.token)?;
        }
        if self.document.text.vertical_mode == Mode::Vertical {
            self.build_page()?;
        }
        Ok(())
    }
}
#[path = "math_display_layout.rs"]
mod layout;
