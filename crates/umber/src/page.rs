//! Incremental TeX82 build_page/fire_up, with default detached publication.
use crate::{
    Engine, Error, Result,
    execute::GroupKind,
    layout_values::{DimenParam, GlueParam},
    nodes::{BoxData, GlueKind, Kind, List, NodeId},
    numeric::{Glue, GlueOrder, Scaled, badness},
    output::Pages,
    packing::PackSpec,
    scalars::{PageParam, Target},
    text::{Mode, TextState},
    token::*,
};
#[path = "page_insertions.rs"]
mod insertions;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Contents {
    #[default]
    Empty,
    InsertsOnly,
    BoxThere,
}
const AWFUL: i32 = 0x3fff_ffff;
#[derive(Clone, Copy)]
pub(crate) struct Measurements {
    pub contents: Contents,
    pub totals: [i32; 8],
    pub max_depth: i32,
    pub best_break: NodeId,
    pub best_previous: NodeId,
    pub best_size: i32,
    pub least_cost: i32,
    pub last_glue: Option<Glue>,
    pub last_penalty: i32,
    pub last_kern: i32,
}
impl Default for Measurements {
    fn default() -> Self {
        Self {
            contents: Contents::Empty,
            totals: [0; 8],
            max_depth: 0,
            best_break: NodeId::NULL,
            best_previous: NodeId::NULL,
            best_size: 0,
            least_cost: AWFUL,
            last_glue: None,
            last_penalty: 0,
            last_kern: 0,
        }
    }
}
pub(crate) struct OutputFrame {
    pub outer: TextState,
    pub level: u16,
}
pub(crate) struct PageState {
    pub current: List,
    pub state: Measurements,
    pub output: Pages,
    pub dead_cycles: i32,
    pub insert_penalties: i32,
    classes: crate::value_metadata::Buffer<insertions::Class>,
}
impl Default for PageState {
    fn default() -> Self {
        Self {
            current: List::default(),
            state: Measurements::default(),
            output: Pages::default(),
            dead_cycles: 0,
            insert_penalties: 0,
            classes: crate::value_metadata::Buffer::new(&std::rc::Rc::default()),
        }
    }
}
impl PageState {
    pub fn snapshot(&self) -> Self {
        Self {
            current: self.current.snapshot(),
            state: self.state,
            output: self.output.clone(),
            dead_cycles: self.dead_cycles,
            insert_penalties: self.insert_penalties,
            classes: self.classes.clone(),
        }
    }
    pub fn quantity(&self, index: usize) -> Scaled {
        if self.state.contents != Contents::Empty {
            Scaled(self.state.totals[index])
        } else if index == 0 {
            Scaled::MAX
        } else {
            Scaled::ZERO
        }
    }
}
impl Engine<'_> {
    pub(crate) fn build_page(&mut self) -> Result<()> {
        while self.document.output_frame.is_none() && !self.document.text.vertical.is_empty() {
            let p = self.document.text.vertical.head();
            let node = self.document.nodes.get(p);
            let state = &mut self.document.page.state;
            state.last_glue = None;
            state.last_penalty = 0;
            state.last_kern = 0;
            match node.kind {
                Kind::Glue { .. } | Kind::Leader(_) => {
                    state.last_glue = Some(self.document.nodes.glue(&node.kind).unwrap().0)
                }
                Kind::Penalty(pi) => state.last_penalty = pi,
                Kind::Kern { width, .. } => state.last_kern = width.0,
                _ => {}
            }
            let mut penalty = None;
            match &node.kind {
                Kind::Box(b) => {
                    let b = self.document.nodes.box_data(b);
                    let (height, depth) = (b.height.0, b.depth.0);
                    if state.contents != Contents::BoxThere {
                        self.start_page(height)?;
                        continue;
                    }
                    state.totals[1] = state.totals[1]
                        .wrapping_add(state.totals[7])
                        .wrapping_add(height);
                    state.totals[7] = depth;
                }
                Kind::Rule { height, depth, .. } => {
                    let (height, depth) = (height.0, depth.0);
                    if state.contents != Contents::BoxThere {
                        self.start_page(height)?;
                        continue;
                    }
                    state.totals[1] = state.totals[1]
                        .wrapping_add(state.totals[7])
                        .wrapping_add(height);
                    state.totals[7] = depth;
                }
                Kind::Glue { .. } | Kind::Leader(_) => {
                    if state.contents != Contents::BoxThere {
                        self.discard_contribution();
                        continue;
                    }
                    if !self.document.page.current.is_empty()
                        && matches!(
                            self.document
                                .nodes
                                .get(self.document.page.current.tail())
                                .kind,
                            Kind::Box(_)
                                | Kind::Rule { .. }
                                | Kind::Insertion(_)
                                | Kind::Language(_)
                        )
                    {
                        penalty = Some(0);
                    }
                }
                Kind::Kern { .. } => {
                    if state.contents != Contents::BoxThere {
                        self.discard_contribution();
                        continue;
                    }
                    if node.next.is_null() {
                        return Ok(());
                    }
                    if self.document.nodes.get(node.next).kind.is_glue() {
                        penalty = Some(0);
                    }
                }
                Kind::Penalty(pi) => {
                    if state.contents != Contents::BoxThere {
                        self.discard_contribution();
                        continue;
                    }
                    penalty = Some(*pi);
                }
                Kind::Insertion(_) => self.consider_insertion(p)?,
                Kind::Language(_) => {}
                _ => return Err(Error::Unsupported),
            }
            if let Some(pi) = penalty.filter(|&pi| pi < 10000)
                && self.page_break_cost(p, pi)
            {
                self.fire_up(p)?;
                continue;
            }
            let state = &mut self.document.page.state;
            match self.document.nodes.get(p).kind {
                Kind::Glue { .. } | Kind::Leader(_) => {
                    let spec = self
                        .document
                        .nodes
                        .glue(&self.document.nodes.get(p).kind)
                        .unwrap()
                        .0;
                    if spec.shrink.0 != 0 && spec.shrink_order != GlueOrder::Normal {
                        return Err(Error::Unsupported);
                    }
                    let i = 2 + spec.stretch_order as usize;
                    state.totals[i] = state.totals[i].wrapping_add(spec.stretch.0);
                    state.totals[6] = state.totals[6].wrapping_add(spec.shrink.0);
                    state.totals[1] = state.totals[1]
                        .wrapping_add(state.totals[7])
                        .wrapping_add(spec.width.0);
                    state.totals[7] = 0;
                }
                Kind::Kern { width, .. } => {
                    state.totals[1] = state.totals[1]
                        .wrapping_add(state.totals[7])
                        .wrapping_add(width.0);
                    state.totals[7] = 0;
                }
                _ => {}
            }
            if state.totals[7] > state.max_depth {
                state.totals[1] = state.totals[1]
                    .wrapping_add(state.totals[7])
                    .wrapping_sub(state.max_depth);
                state.totals[7] = state.max_depth;
            }
            let node = self
                .document
                .nodes
                .pop_front(&mut self.document.text.vertical)
                .unwrap();
            self.document
                .nodes
                .append(&mut self.document.page.current, node);
        }
        Ok(())
    }
    fn start_page(&mut self, first_height: i32) -> Result<()> {
        let s = &mut self.document.page.state;
        if s.contents == Contents::Empty {
            self.freeze_page(Contents::BoxThere);
        }
        let s = &mut self.document.page.state;
        s.contents = Contents::BoxThere;
        let mut spec = self.document.layout_values.glues[GlueParam::Top.index()].value;
        spec.width = Scaled(spec.width.0.saturating_sub(first_height).max(0));
        let glue = self.document.nodes.alloc(Kind::Glue {
            spec,
            subtype: GlueKind::TopSkip,
        })?;
        let mut prefix = List::default();
        self.document.nodes.append(&mut prefix, glue);
        self.document.nodes.splice(
            &mut prefix,
            std::mem::take(&mut self.document.text.vertical),
        );
        self.document.text.vertical = prefix;
        Ok(())
    }
    fn freeze_page(&mut self, contents: Contents) {
        let s = &mut self.document.page.state;
        s.contents = contents;
        s.totals = [0; 8];
        s.totals[0] = self.document.layout_values.dimens[DimenParam::VSize.index()]
            .value
            .0;
        s.max_depth = self.document.layout_values.dimens[DimenParam::MaxDepth.index()]
            .value
            .0;
        s.least_cost = AWFUL;
    }
    fn discard_contribution(&mut self) {
        let node = self
            .document
            .nodes
            .pop_front(&mut self.document.text.vertical)
            .unwrap();
        self.document
            .nodes
            .release_node(node, &mut self.page_release);
    }
    fn page_break_cost(&mut self, p: NodeId, pi: i32) -> bool {
        let s = &mut self.document.page.state;
        let b = if s.totals[1] < s.totals[0] {
            if s.totals[3..6].iter().any(|&x| x != 0) {
                0
            } else {
                badness(
                    Scaled(s.totals[0].wrapping_sub(s.totals[1])),
                    Scaled(s.totals[2]),
                )
            }
        } else if s.totals[1].wrapping_sub(s.totals[0]) > s.totals[6] {
            AWFUL
        } else {
            badness(
                Scaled(s.totals[1].wrapping_sub(s.totals[0])),
                Scaled(s.totals[6]),
            )
        };
        let cost = if b == AWFUL {
            b
        } else if pi <= -10000 {
            pi
        } else if b < 10000 {
            b.wrapping_add(pi)
                .wrapping_add(self.document.page.insert_penalties)
        } else {
            100000
        };
        let cost = if self.document.page.insert_penalties >= 10000 {
            AWFUL
        } else {
            cost
        };
        if cost <= s.least_cost {
            s.best_break = p;
            s.best_previous = self.document.page.current.tail();
            s.best_size = s.totals[0];
            s.least_cost = cost;
            for class in &mut *self.document.page.classes {
                class.best = class.last;
            }
        }
        cost == AWFUL || pi <= -10000
    }
    fn fire_up(&mut self, candidate: NodeId) -> Result<()> {
        let s = self.document.page.state;
        let penalty = match self.document.nodes.get(s.best_break).kind {
            Kind::Penalty(pi) => pi,
            _ => 10000,
        };
        self.update_integer(Target::page(PageParam::OutputPenalty), true, |_| {
            Ok(penalty)
        })?;
        if let Kind::Penalty(pi) = self.document.nodes.kind_mut(s.best_break) {
            *pi = 10000;
        }
        if !self.document.boxes.head(255).is_null() {
            return Err(Error::Unsupported);
        }
        self.document.page.insert_penalties = 0;
        self.prepare_insertions()?;
        if candidate != s.best_break {
            let rest = self
                .document
                .nodes
                .split_after(&mut self.document.page.current, s.best_previous);
            let mut rest = rest;
            self.document
                .nodes
                .splice(&mut rest, std::mem::take(&mut self.document.text.vertical));
            self.document.text.vertical = rest;
        }
        self.commit_insertions()?;
        let list = std::mem::take(&mut self.document.page.current);
        let page = self.vpackage(list, PackSpec::To(Scaled(s.best_size)), Scaled(s.max_depth))?;
        // Start-new-page resets caches/depth, but raw previous totals are retained.
        let state = &mut self.document.page.state;
        state.contents = Contents::Empty;
        state.max_depth = 0;
        state.totals[7] = 0;
        state.last_glue = None;
        state.last_penalty = 0;
        state.last_kern = 0;
        state.best_break = NodeId::NULL;
        state.best_previous = NodeId::NULL;
        self.document.page.classes.clear();
        self.install_output_box(page);
        if let Some(owner) = self.document.token_registers[OUTPUT_ROUTINE].owner.clone() {
            if self.document.page.dead_cycles
                >= self.document.scalars[Target::MAX_DEAD_CYCLES].value
            {
                return Err(Error::Unsupported);
            }
            self.document.page.dead_cycles = self.document.page.dead_cycles.wrapping_add(1);
            let level = self.document.level.checked_add(1).ok_or(Error::Capacity)?;
            let outer = std::mem::replace(
                &mut self.document.text,
                TextState {
                    mode: Mode::InternalVertical,
                    vertical_mode: Mode::InternalVertical,
                    ..TextState::default()
                },
            );
            // The frame owns the suspended contributions even if the following
            // group/scanner operations fail. Restore abandons the whole root.
            self.document.output_frame = Some(OutputFrame { outer, level });
            self.push_tokens(owner, 0)?;
            self.document
                .sources
                .last_mut()
                .and_then(Source::cursor_mut)
                .expect("nonempty output")
                .role = SourceRole::Output;
            self.open_group(GroupKind::Output)?;
            self.normal_paragraph()?;
            self.opening_brace()?;
            Ok(())
        } else {
            let page = self.take_output_box();
            self.publish_page(page)
        }
    }
    pub(crate) fn prepare_output_end(&mut self, kind: GroupKind) -> Result<bool> {
        let Some(frame) = &self.document.output_frame else {
            return Ok(false);
        };
        if frame.level != self.document.level {
            return Ok(false);
        }
        if kind != GroupKind::Simple {
            return Err(Error::Group);
        }
        if !self.current_source_exhausted()
            || !self
                .document
                .sources
                .last()
                .is_some_and(|s| matches!(s.role(), SourceRole::Output | SourceRole::BackedUp))
        {
            return Err(Error::Syntax);
        }
        self.pop_source();
        self.end_graf()?;
        Ok(true)
    }
    pub(crate) fn finish_output_group(&mut self) -> Result<()> {
        self.document.page.insert_penalties = 0;
        if !self.document.boxes.head(255).is_null() {
            return Err(Error::Unsupported);
        }
        let frame = self.document.output_frame.take().expect("output group");
        let material = std::mem::take(&mut self.document.text.vertical);
        self.document.text = frame.outer;
        let mut material = material;
        self.document.nodes.splice(
            &mut material,
            std::mem::take(&mut self.document.text.vertical),
        );
        self.document.text.vertical = material;
        self.build_page()
    }
    pub(crate) fn end_pages(&mut self, read: Read) -> Result<bool> {
        if self.document.page.current.is_empty()
            && self.document.text.vertical.is_empty()
            && self.document.page.dead_cycles == 0
        {
            return Ok(true);
        }
        self.back_read(read)?;
        let width = self.document.layout_values.dimens[DimenParam::HSize.index()].value;
        let node = self.document.nodes.alloc_box(BoxData {
            width,
            ..BoxData::default()
        })?;
        self.document
            .nodes
            .append(&mut self.document.text.vertical, node);
        for kind in [
            Kind::Glue {
                spec: Glue {
                    stretch: Scaled::ONE,
                    stretch_order: GlueOrder::Fill,
                    ..Glue::default()
                },
                subtype: GlueKind::Normal,
            },
            Kind::Penalty(-1_073_741_824),
        ] {
            let node = self.document.nodes.alloc(kind)?;
            self.document
                .nodes
                .append(&mut self.document.text.vertical, node);
        }
        self.build_page()?;
        Ok(false)
    }
}
