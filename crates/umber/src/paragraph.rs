//! Paragraph parameters, pooled shape roots and reusable search lifetime.
use crate::{
    Engine, Error, Result,
    execute::Save,
    layout_values::{DimenParam, GlueParam, LineShape, ShapeSlot},
    nodes::{GlueKind, Kind, Node, NodeId, ReleaseScratch},
    numeric::{Glue, GlueOrder, Scaled},
    scalars::{ParagraphParam as P, Target},
    tables::Table,
};

pub(crate) type Widths = [i32; 6];
pub(crate) const AWFUL: i32 = 0x3fff_ffff;
pub(crate) const NONE: usize = usize::MAX;
#[derive(Clone, Copy)]
pub(crate) struct Active {
    pub line: i32,
    pub fitness: u8,
    pub hyphenated: bool,
    pub passive: usize,
    pub demerits: i32,
}
#[derive(Clone, Copy)]
pub(crate) enum RecordKind {
    Active(Active),
    Delta(Widths),
    Free,
}
#[derive(Clone, Copy)]
pub(crate) struct Record {
    pub next: usize,
    pub kind: RecordKind,
}
#[derive(Clone, Copy)]
pub(crate) struct Passive {
    pub breakpoint: NodeId,
    pub previous: usize,
}
#[derive(Default)]
pub(crate) struct Scratch {
    pub records: Vec<Record>,
    pub passive: Vec<Passive>,
    pub breaks: Vec<NodeId>,
    pub free: usize,
    pub release: ReleaseScratch,
    pub shapes: Option<Table<LineShape>>,
}
impl Scratch {
    pub fn reset(&mut self) {
        self.records.clear();
        self.passive.clear();
        self.breaks.clear();
        self.free = NONE;
    }
    pub fn alloc(&mut self, kind: RecordKind, next: usize) -> usize {
        let record = Record { kind, next };
        if self.free != NONE {
            let id = self.free;
            self.free = self.records[id].next;
            self.records[id] = record;
            id
        } else {
            self.records.push(record);
            self.records.len() - 1
        }
    }
    pub fn free(&mut self, id: usize) {
        self.records[id] = Record {
            next: self.free,
            kind: RecordKind::Free,
        };
        self.free = id;
    }
}
pub(crate) struct Parameters {
    pub integers: [i32; 14],
    pub background: Widths,
    pub left: Glue,
    pub right: Glue,
    pub emergency: i32,
    pub first: LineShape,
    pub second: LineShape,
    pub last_special: i32,
    pub easy: i32,
    pub shape: Option<Table<LineShape>>,
}
impl Parameters {
    pub fn integer(&self, p: P) -> i32 {
        self.integers[p as usize]
    }
    pub fn line(&self, line: i32) -> LineShape {
        if line > self.last_special {
            self.second
        } else if let Some(shape) = &self.shape {
            shape[(line - 1) as usize]
        } else {
            self.first
        }
    }
    pub(crate) fn capture(engine: &Engine<'_>) -> Result<Self> {
        let d = &engine.document;
        let integers = std::array::from_fn(|i| d.scalars[Target::parameter(9 + i)].value);
        let left = d.layout_values.glues[GlueParam::Left.index()].value;
        let right = d.layout_values.glues[GlueParam::Right.index()].value;
        check_shrink(left)?;
        check_shrink(right)?;
        let mut background = [0; 6];
        add_glue(&mut background, left, 1);
        add_glue(&mut background, right, 1);
        let hsize = d.layout_values.dimens[DimenParam::HSize.index()].value;
        let hang = d.layout_values.dimens[DimenParam::HangIndent.index()].value;
        let after = integers[P::HangAfter as usize];
        let shape = d.layout_values.shape.lines.clone();
        let normal = LineShape {
            indent: Scaled::ZERO,
            width: hsize,
        };
        let (first, second, last_special) = if let Some(lines) = &shape {
            (normal, lines[lines.len() - 1], lines.len() as i32 - 1)
        } else if hang.0 == 0 {
            (normal, normal, 0)
        } else {
            let special = LineShape {
                indent: Scaled(hang.0.max(0)),
                width: Scaled(
                    hsize
                        .0
                        .wrapping_sub(hang.0.checked_abs().ok_or(Error::Overflow)?),
                ),
            };
            if after < 0 {
                (special, normal, after.checked_abs().ok_or(Error::Overflow)?)
            } else {
                (normal, special, after)
            }
        };
        let easy = if integers[P::Looseness as usize] == 0 {
            last_special
        } else {
            i32::MAX - 1
        };
        Ok(Self {
            integers,
            background,
            left,
            right,
            emergency: d.layout_values.dimens[DimenParam::EmergencyStretch.index()]
                .value
                .0,
            first,
            second,
            last_special,
            easy,
            shape,
        })
    }
}
pub(crate) fn check_shrink(glue: Glue) -> Result<()> {
    if glue.shrink.0 != 0 && glue.shrink_order != GlueOrder::Normal {
        Err(Error::Unsupported)
    } else {
        Ok(())
    }
}
pub(crate) fn add_glue(width: &mut Widths, glue: Glue, sign: i32) {
    width[0] = width[0].wrapping_add(glue.width.0.wrapping_mul(sign));
    let order = 1 + glue.stretch_order as usize;
    width[order] = width[order].wrapping_add(glue.stretch.0.wrapping_mul(sign));
    width[5] = width[5].wrapping_add(glue.shrink.0.wrapping_mul(sign));
}
pub(crate) fn plus(a: Widths, b: Widths) -> Widths {
    std::array::from_fn(|i| a[i].wrapping_add(b[i]))
}
pub(crate) fn minus(a: Widths, b: Widths) -> Widths {
    std::array::from_fn(|i| a[i].wrapping_sub(b[i]))
}
impl Engine<'_> {
    pub(crate) fn shape_assignment(&mut self, global: bool) -> Result<()> {
        self.scan_equals()?;
        let count = self.scan_int()?;
        let lines = if count > 0 {
            let mut lines = self
                .paragraph
                .shapes
                .get_or_insert_with(|| Table::new(0))
                .clone();
            for index in 0..count {
                lines[index as usize] = LineShape {
                    indent: self.scan_dimen()?,
                    width: self.scan_dimen()?,
                };
            }
            Some(lines)
        } else {
            None
        };
        self.assign_shape(lines, global);
        Ok(())
    }
    fn assign_shape(&mut self, lines: Option<Table<LineShape>>, global: bool) {
        let old = std::mem::replace(
            &mut self.document.layout_values.shape,
            ShapeSlot {
                lines,
                level: if global { 0 } else { self.document.level },
            },
        );
        if !global && self.document.level > 0 && old.level != self.document.level {
            self.document.save.push(Save::Shape(old));
        }
    }
    pub(crate) fn normal_paragraph(&mut self) -> Result<()> {
        for (p, value) in [(P::Looseness, 0), (P::HangAfter, 1)] {
            self.update_integer(Target::paragraph(p), false, |_| Ok(value))?;
        }
        self.assign_dimen(DimenParam::HangIndent.index(), Scaled::ZERO, false);
        if self.document.layout_values.shape.lines.is_some() {
            self.assign_shape(None, false);
        }
        Ok(())
    }
    pub(crate) fn break_paragraph(&mut self) -> Result<()> {
        let parameters = Parameters::capture(self)?;
        // All contents remain in a persistent owned root even when validation or
        // allocation fails. Only completed line construction transfers ownership.
        let tail = self.document.text.horizontal.tail();
        let mut p = self.document.text.horizontal.head();
        while !p.is_null() {
            let n = self.document.nodes.get(p);
            if let Kind::Glue { spec, .. } = n.kind
                && p != tail
            {
                check_shrink(spec)?;
            }
            p = n.next;
        }
        if matches!(self.document.nodes.get(tail).kind, Kind::Glue { .. }) {
            *self.document.nodes.kind_mut(tail) = Kind::Penalty(10000);
        } else {
            self.append_horizontal(Kind::Penalty(10000))?;
        }
        let fill = self.document.layout_values.glues[GlueParam::ParFill.index()].value;
        check_shrink(fill)?;
        self.append_horizontal(Kind::Glue {
            spec: fill,
            subtype: GlueKind::ParFillSkip,
        })?;
        let mut scratch = std::mem::take(&mut self.paragraph);
        scratch.reset();
        let result = (|| {
            let best = crate::line_break::find_breaks(self, &parameters, &mut scratch)?;
            self.construct_lines(&parameters, &mut scratch, best)
        })();
        scratch.reset();
        self.paragraph = scratch;
        result
    }
    pub(crate) fn node_width(&self, node: &Node) -> Result<i32> {
        Ok(match &node.kind {
            Kind::Character { font, character }
            | Kind::Ligature {
                font, character, ..
            } => {
                self.character_metrics(*font, *character)
                    .ok_or(Error::Font)?
                    .width
                    .0
            }
            Kind::Kern { width, .. } | Kind::Rule { width, .. } => width.0,
            Kind::Box(b) => self.document.nodes.box_data(b).width.0,
            _ => return Err(Error::Unsupported),
        })
    }
}
