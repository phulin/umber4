//! Incremental TeX82 build_page/fire_up, with default detached publication.
use crate::{
    Engine, Error, Result,
    layout_values::{DimenParam, GlueParam},
    nodes::{BoxData, GlueKind, Kind, List, NodeId},
    numeric::{Glue, GlueOrder, Scaled, badness},
    output::Pages,
    packing::PackSpec,
    scalars::{PageParam, Target},
    token::*,
};
const AWFUL: i32 = 0x3fff_ffff;
#[derive(Clone, Copy)]
pub(crate) struct Measurements {
    pub started: bool,
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
            started: false,
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
#[derive(Default)]
pub(crate) struct PageState {
    pub current: List,
    pub state: Measurements,
    pub output: Pages,
}
impl PageState {
    pub fn snapshot(&self) -> Self {
        Self {
            current: self.current.snapshot(),
            state: self.state,
            output: self.output.clone(),
        }
    }
    pub fn quantity(&self, index: usize) -> Scaled {
        if self.state.started {
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
        while !self.document.text.vertical.is_empty() {
            let p = self.document.text.vertical.head();
            let node = self.document.nodes.get(p);
            let state = &mut self.document.page.state;
            state.last_glue = None;
            state.last_penalty = 0;
            state.last_kern = 0;
            match node.kind {
                Kind::Glue { spec, .. } => state.last_glue = Some(spec),
                Kind::Penalty(pi) => state.last_penalty = pi,
                Kind::Kern { width, .. } => state.last_kern = width.0,
                _ => {}
            }
            let mut penalty = None;
            match &node.kind {
                Kind::Box(b) => {
                    let b = self.document.nodes.box_data(b);
                    let (height, depth) = (b.height.0, b.depth.0);
                    if !state.started {
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
                    if !state.started {
                        self.start_page(height)?;
                        continue;
                    }
                    state.totals[1] = state.totals[1]
                        .wrapping_add(state.totals[7])
                        .wrapping_add(height);
                    state.totals[7] = depth;
                }
                Kind::Glue { .. } => {
                    if !state.started {
                        self.discard_contribution();
                        continue;
                    }
                    if !self.document.page.current.is_empty()
                        && matches!(
                            self.document
                                .nodes
                                .get(self.document.page.current.tail())
                                .kind,
                            Kind::Box(_) | Kind::Rule { .. }
                        )
                    {
                        penalty = Some(0);
                    }
                }
                Kind::Kern { .. } => {
                    if !state.started {
                        self.discard_contribution();
                        continue;
                    }
                    if node.next.is_null() {
                        return Ok(());
                    }
                    if matches!(self.document.nodes.get(node.next).kind, Kind::Glue { .. }) {
                        penalty = Some(0);
                    }
                }
                Kind::Penalty(pi) => {
                    if !state.started {
                        self.discard_contribution();
                        continue;
                    }
                    penalty = Some(*pi);
                }
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
                Kind::Glue { spec, .. } => {
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
        s.started = true;
        s.totals = [0; 8];
        s.totals[0] = self.document.layout_values.dimens[DimenParam::VSize.index()]
            .value
            .0;
        s.max_depth = self.document.layout_values.dimens[DimenParam::MaxDepth.index()]
            .value
            .0;
        s.least_cost = AWFUL;
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
        } else {
            100000
        };
        if cost <= s.least_cost {
            s.best_break = p;
            s.best_previous = self.document.page.current.tail();
            s.best_size = s.totals[0];
            s.least_cost = cost;
        }
        cost == AWFUL || pi <= -10000
    }
    fn fire_up(&mut self, candidate: NodeId) -> Result<()> {
        let s = self.document.page.state;
        let penalty = match self.document.nodes.kind_mut(s.best_break) {
            Kind::Penalty(pi) => {
                let old = *pi;
                *pi = 10000;
                old
            }
            _ => 10000,
        };
        self.update_integer(Target::page(PageParam::OutputPenalty), true, |_| {
            Ok(penalty)
        })?;
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
        let list = std::mem::take(&mut self.document.page.current);
        let page = self.vpackage(list, PackSpec::To(Scaled(s.best_size)), Scaled(s.max_depth))?;
        // Start-new-page resets caches/depth, but raw previous totals are retained.
        let state = &mut self.document.page.state;
        state.started = false;
        state.max_depth = 0;
        state.totals[7] = 0;
        state.last_glue = None;
        state.last_penalty = 0;
        state.last_kern = 0;
        state.best_break = NodeId::NULL;
        state.best_previous = NodeId::NULL;
        self.publish_page(page, penalty)
    }
    pub(crate) fn end_pages(&mut self, read: Read) -> Result<bool> {
        if self.document.page.current.is_empty() && self.document.text.vertical.is_empty() {
            return Ok(true);
        }
        self.back_read(read);
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
