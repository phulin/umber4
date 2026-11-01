//! TeX try_break over recycled index-linked active and delta records.
use crate::{
    Engine, Result,
    nodes::{Kind, NodeId},
    numeric::{Scaled, badness},
    paragraph::{
        AWFUL, Active, NONE, Parameters, Passive, RecordKind, Scratch, Widths, add_glue, minus,
        plus,
    },
    scalars::ParagraphParam as P,
};

pub(crate) struct Search<'a, 's> {
    pub parameters: &'a Parameters,
    pub scratch: &'s mut Scratch,
    pub width: Widths,
    pub background: Widths,
    pub point: NodeId,
    pub threshold: i32,
    pub final_pass: bool,
    pub disc_width: i32,
}
impl Search<'_, '_> {
    fn delta(&self, id: usize) -> Option<Widths> {
        if let RecordKind::Delta(w) = self.scratch.records[id].kind {
            Some(w)
        } else {
            None
        }
    }
    fn active(&self, id: usize) -> Active {
        match self.scratch.records[id].kind {
            RecordKind::Active(a) => a,
            _ => unreachable!(),
        }
    }
    fn set_next(&mut self, id: usize, next: usize) {
        self.scratch.records[id].next = next;
    }
    fn next(&self, id: usize) -> usize {
        self.scratch.records[id].next
    }
    fn break_width(&self, engine: &Engine<'_>, hyphenated: bool) -> Result<Widths> {
        let mut width = self.background;
        let mut s = self.point;
        if hyphenated && !s.is_null() {
            let (post, count) = match &engine.document.nodes.get(s).kind {
                Kind::Discretionary {
                    post,
                    replace_count,
                    ..
                } => (post.head(), *replace_count),
                _ => unreachable!(),
            };
            let mut v = s;
            for _ in 0..count {
                v = engine.document.nodes.get(v).next;
                width[0] = width[0].wrapping_sub(engine.node_width(engine.document.nodes.get(v))?);
            }
            let mut p = post;
            while !p.is_null() {
                let node = engine.document.nodes.get(p);
                width[0] = width[0].wrapping_add(engine.node_width(node)?);
                p = node.next;
            }
            width[0] = width[0].wrapping_add(self.disc_width);
            s = if post.is_null() {
                engine.document.nodes.get(v).next
            } else {
                NodeId::NULL
            };
        }
        while !s.is_null() {
            let node = engine.document.nodes.get(s);
            match node.kind {
                Kind::Glue { spec, .. } => add_glue(&mut width, spec, -1),
                Kind::Penalty(_) => {}
                Kind::Kern {
                    width: w,
                    explicit: true,
                } => width[0] = width[0].wrapping_sub(w.0),
                _ => break,
            }
            s = node.next;
        }
        Ok(width)
    }
    /// The loop and insert/deactivate ordering intentionally follow WEB. In
    /// particular, equal demerits choose the later predecessor in active order.
    pub fn try_break(
        &mut self,
        engine: &Engine<'_>,
        mut penalty: i32,
        hyphenated: bool,
    ) -> Result<()> {
        if penalty >= 10000 {
            return Ok(());
        }
        if penalty <= -10000 {
            penalty = -10000;
        }
        let mut previous = 0;
        let mut previous_previous = 0;
        let mut old_line = 0;
        let mut current = self.width;
        let mut minimum = AWFUL;
        let mut minimal = [AWFUL; 4];
        let mut best_place = [NONE; 4];
        let mut best_line = [0; 4];
        let mut computed_break = None;
        let mut line_width = 0;
        loop {
            let mut r = self.next(previous);
            if let Some(delta) = self.delta(r) {
                current = plus(current, delta);
                previous_previous = previous;
                previous = r;
                continue;
            }
            let active = self.active(r);
            let line = active.line;
            if line > old_line {
                if minimum < AWFUL && (old_line != self.parameters.easy || r == 0) {
                    let bw = if let Some(w) = computed_break {
                        w
                    } else {
                        let w = self.break_width(engine, hyphenated)?;
                        computed_break = Some(w);
                        w
                    };
                    if let Some(delta) = self.delta(previous) {
                        self.scratch.records[previous].kind =
                            RecordKind::Delta(plus(minus(delta, current), bw));
                    } else if previous == 0 {
                        self.width = bw;
                    } else {
                        let q = self.scratch.alloc(RecordKind::Delta(minus(bw, current)), r);
                        self.set_next(previous, q);
                        previous_previous = previous;
                        previous = q;
                    }
                    let cutoff = (i64::from(minimum)
                        + i64::from(self.parameters.integer(P::AdjDemerits)).abs())
                    .min(i64::from(AWFUL - 1)) as i32;
                    for fitness in 0..4 {
                        if minimal[fitness] <= cutoff {
                            let passive = self.scratch.passive.len();
                            self.scratch.passive.push(Passive {
                                breakpoint: self.point,
                                previous: best_place[fitness],
                            });
                            let q = self.scratch.alloc(
                                RecordKind::Active(Active {
                                    line: best_line[fitness] + 1,
                                    fitness: fitness as u8,
                                    hyphenated,
                                    passive,
                                    demerits: minimal[fitness],
                                }),
                                r,
                            );
                            self.set_next(previous, q);
                            previous = q;
                        }
                        minimal[fitness] = AWFUL;
                    }
                    minimum = AWFUL;
                    if r != 0 {
                        let q = self.scratch.alloc(RecordKind::Delta(minus(current, bw)), r);
                        self.set_next(previous, q);
                        previous_previous = previous;
                        previous = q;
                    }
                }
                if r == 0 {
                    return Ok(());
                }
                if line > self.parameters.easy {
                    line_width = self.parameters.second.width.0;
                    old_line = i32::MAX - 1;
                } else {
                    line_width = self.parameters.line(line).width.0;
                    old_line = line;
                }
            }
            let shortfall = line_width.wrapping_sub(current[0]);
            let (b, fitness) = if shortfall > 0 {
                if current[2] != 0 || current[3] != 0 || current[4] != 0 {
                    (0, 2)
                } else {
                    let b = badness(Scaled(shortfall), Scaled(current[1]));
                    (
                        b,
                        if b > 99 {
                            0
                        } else if b > 12 {
                            1
                        } else {
                            2
                        },
                    )
                }
            } else {
                let b = if -i64::from(shortfall) > i64::from(current[5]) {
                    10001
                } else {
                    badness(Scaled(shortfall.wrapping_neg()), Scaled(current[5]))
                };
                (b, if b > 12 { 3 } else { 2 })
            };
            let mut artificial = false;
            let stays = if b > 10000 || penalty == -10000 {
                artificial =
                    self.final_pass && minimum == AWFUL && self.next(r) == 0 && previous == 0;
                false
            } else {
                true
            };
            let feasible = if stays {
                b <= self.threshold
            } else {
                artificial || b <= self.threshold
            };
            if feasible {
                let mut d = if artificial {
                    0
                } else {
                    let base = i64::from(self.parameters.integer(P::LinePenalty)) + i64::from(b);
                    let mut d = if base.abs() >= 10000 {
                        100_000_000
                    } else {
                        (base * base) as i32
                    };
                    if penalty > 0 {
                        d = d.wrapping_add(penalty * penalty);
                    } else if penalty > -10000 {
                        d = d.wrapping_sub(penalty * penalty);
                    }
                    if hyphenated && active.hyphenated {
                        d = d.wrapping_add(self.parameters.integer(if self.point.is_null() {
                            P::FinalHyphenDemerits
                        } else {
                            P::DoubleHyphenDemerits
                        }));
                    }
                    if (fitness as i32 - i32::from(active.fitness)).abs() > 1 {
                        d = d.wrapping_add(self.parameters.integer(P::AdjDemerits));
                    }
                    d
                };
                d = d.wrapping_add(active.demerits);
                if d <= minimal[fitness] {
                    minimal[fitness] = d;
                    best_place[fitness] = active.passive;
                    best_line[fitness] = line;
                    if d < minimum {
                        minimum = d;
                    }
                }
            }
            if stays {
                previous = r;
                continue;
            }
            self.set_next(previous, self.next(r));
            self.scratch.free(r);
            if previous == 0 {
                r = self.next(0);
                if let Some(delta) = self.delta(r) {
                    self.width = plus(self.width, delta);
                    current = self.width;
                    self.set_next(0, self.next(r));
                    self.scratch.free(r);
                }
            } else if let Some(delta) = self.delta(previous) {
                r = self.next(previous);
                if r == 0 {
                    current = minus(current, delta);
                    self.set_next(previous_previous, 0);
                    self.scratch.free(previous);
                    previous = previous_previous;
                } else if let Some(next_delta) = self.delta(r) {
                    current = plus(current, next_delta);
                    self.scratch.records[previous].kind =
                        RecordKind::Delta(plus(delta, next_delta));
                    self.set_next(previous, self.next(r));
                    self.scratch.free(r);
                }
            }
        }
    }
    pub fn best(&self) -> Option<(usize, i32, i32)> {
        let mut r = self.next(0);
        let mut best = NONE;
        let mut demerits = AWFUL;
        while r != 0 {
            if let RecordKind::Active(a) = self.scratch.records[r].kind
                && a.demerits < demerits
            {
                demerits = a.demerits;
                best = r;
            }
            r = self.next(r);
        }
        if best == NONE {
            return None;
        }
        let base_line = self.active(best).line;
        let requested = self.parameters.integer(P::Looseness);
        let mut actual = 0;
        if requested != 0 {
            r = self.next(0);
            while r != 0 {
                if let RecordKind::Active(a) = self.scratch.records[r].kind {
                    let diff = a.line - base_line;
                    if (diff < actual && requested <= diff) || (diff > actual && requested >= diff)
                    {
                        best = r;
                        actual = diff;
                        demerits = a.demerits;
                    } else if diff == actual && a.demerits < demerits {
                        best = r;
                        demerits = a.demerits;
                    }
                }
                r = self.next(r);
            }
        }
        Some((self.active(best).passive, self.active(best).line, actual))
    }
}
