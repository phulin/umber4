//! Paragraph passes and legal-break traversal over the original nodes.
use crate::{
    Engine, Error, Result,
    line_search::Search,
    nodes::{Kind, NodeId},
    paragraph::{Active, NONE, Parameters, RecordKind, Scratch, add_glue},
    scalars::ParagraphParam as P,
};
pub(crate) fn find_breaks(
    engine: &mut Engine<'_>,
    parameters: &Parameters,
    scratch: &mut Scratch,
) -> Result<i32> {
    let mut threshold = parameters.integer(P::Pretolerance);
    let mut second = threshold < 0;
    if second {
        threshold = parameters.integer(P::Tolerance);
    }
    let mut final_pass = second && parameters.emergency <= 0;
    let mut background = parameters.background;
    loop {
        if second {
            engine.document.hyphenation.begin_pass()?;
        }
        scratch.reset();
        scratch.alloc(
            RecordKind::Active(Active {
                line: i32::MAX,
                fitness: 0,
                hyphenated: true,
                passive: NONE,
                demerits: 0,
            }),
            0,
        );
        let initial = scratch.alloc(
            RecordKind::Active(Active {
                line: engine.document.text.prev_graf + 1,
                fitness: 2,
                hyphenated: false,
                passive: NONE,
                demerits: 0,
            }),
            0,
        );
        scratch.records[0].next = initial;
        let mut search = Search {
            engine,
            parameters,
            scratch,
            width: background,
            background,
            point: NodeId::NULL,
            threshold: threshold.min(10000),
            final_pass,
            disc_width: 0,
        };
        let mut p = engine.document.text.horizontal.head();
        let mut previous = p;
        let mut metrics = engine.metric_reader();
        while !p.is_null() && search.scratch.records[0].next != 0 {
            search.point = p;
            let node = engine.document.nodes.get(p);
            match &node.kind {
                Kind::Character { font, character }
                | Kind::Ligature {
                    font, character, ..
                } => {
                    let width = metrics.get(*font, *character).ok_or(Error::Font)?.width.0;
                    search.width[0] = search.width[0].wrapping_add(width);
                }
                Kind::Box(_) | Kind::Rule { .. } => {
                    search.width[0] = search.width[0].wrapping_add(engine.node_width(node)?)
                }
                Kind::Glue { spec, .. } => {
                    let before = &engine.document.nodes.get(previous).kind;
                    if matches!(
                        before,
                        Kind::Character { .. }
                            | Kind::Ligature { .. }
                            | Kind::Box(_)
                            | Kind::Rule { .. }
                            | Kind::Discretionary { .. }
                            | Kind::Kern {
                                explicit: false,
                                ..
                            }
                    ) {
                        search.try_break(0, false)?;
                    }
                    add_glue(&mut search.width, *spec, 1);
                }
                Kind::Kern { width, explicit } => {
                    if *explicit
                        && !node.next.is_null()
                        && matches!(engine.document.nodes.get(node.next).kind, Kind::Glue { .. })
                    {
                        search.try_break(0, false)?;
                    }
                    search.width[0] = search.width[0].wrapping_add(width.0);
                }
                Kind::Penalty(penalty) => search.try_break(*penalty, false)?,
                Kind::Discretionary {
                    pre, replace_count, ..
                } => {
                    let mut q = pre.head();
                    let mut width = 0i32;
                    while !q.is_null() {
                        let node = engine.document.nodes.get(q);
                        width = width.wrapping_add(engine.node_width(node)?);
                        q = node.next;
                    }
                    search.disc_width = width;
                    search.width[0] = search.width[0].wrapping_add(width);
                    search.try_break(
                        parameters.integer(if pre.is_empty() {
                            P::ExHyphenPenalty
                        } else {
                            P::HyphenPenalty
                        }),
                        true,
                    )?;
                    search.width[0] = search.width[0].wrapping_sub(width);
                    let mut next = node.next;
                    for _ in 0..*replace_count {
                        let node = engine.document.nodes.get(next);
                        search.width[0] = search.width[0].wrapping_add(engine.node_width(node)?);
                        next = node.next;
                    }
                    previous = p;
                    p = next;
                    continue;
                }
                Kind::Free => unreachable!(),
            }
            previous = p;
            p = node.next;
        }
        if p.is_null() {
            search.point = NodeId::NULL;
            search.try_break(-10000, true)?;
            if let Some((mut passive, line, actual)) = search.best()
                && (parameters.integer(P::Looseness) == 0
                    || actual == parameters.integer(P::Looseness)
                    || final_pass)
            {
                while passive != NONE {
                    let record = search.scratch.passive[passive];
                    search.scratch.breaks.push(record.breakpoint);
                    passive = record.previous;
                }
                search.scratch.breaks.reverse();
                return Ok(line);
            }
        }
        if !second {
            threshold = parameters.integer(P::Tolerance);
            second = true;
            final_pass = parameters.emergency <= 0;
        } else {
            background[1] = background[1].wrapping_add(parameters.emergency);
            final_pass = true;
        }
    }
}
