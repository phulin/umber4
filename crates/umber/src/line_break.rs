//! Paragraph passes and legal-break traversal over the original nodes.
use crate::nodes::KernKind;
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
        let mut previous_permits_glue_break = false;
        let mut metrics =
            crate::fonts::MetricReader::new(&engine.document.fonts, &engine.font_catalog);
        let mut language = engine.document.text.initial_language;
        let mut auto_breaking = true;
        while !p.is_null() && search.scratch.records[0].next != 0 {
            search.point = p;
            let node = engine.document.nodes.get(p);
            let mut next = node.next;
            previous_permits_glue_break = match &node.kind {
                Kind::Character { font, character }
                | Kind::Ligature {
                    font, character, ..
                } => {
                    let width = metrics.get(*font, *character).ok_or(Error::Font)?.width.0;
                    search.width[0] = search.width[0].wrapping_add(width);
                    true
                }
                Kind::Box(_) | Kind::Rule { .. } => {
                    search.width[0] = search.width[0].wrapping_add(engine.node_width(node)?);
                    true
                }
                Kind::Glue { .. } | Kind::Leader(_) => {
                    let spec = engine.document.nodes.glue(&node.kind).unwrap().0;
                    if auto_breaking && previous_permits_glue_break {
                        search.try_break(engine, 0, false)?;
                    }
                    add_glue(&mut search.width, spec, 1);
                    if second && auto_breaking {
                        crate::hyphenation_render::after_glue(
                            p,
                            &mut engine.document.nodes,
                            &mut engine.document.text.horizontal,
                            &crate::hyphenation_render::Inputs {
                                scalars: &engine.document.scalars,
                                fonts: &engine.document.fonts,
                                catalog: &engine.font_catalog,
                                tables: &engine.document.hyphenation,
                            },
                            &mut language,
                            &mut search.scratch.hyphenation,
                        )?;
                        next = engine.document.nodes.get(p).next;
                    }
                    false
                }
                Kind::Kern { width, kind } => {
                    let explicit = *kind == KernKind::Explicit;
                    if explicit
                        && auto_breaking
                        && !node.next.is_null()
                        && engine.document.nodes.get(node.next).kind.is_glue()
                    {
                        search.try_break(engine, 0, false)?;
                    }
                    search.width[0] = search.width[0].wrapping_add(width.0);
                    !explicit
                }
                Kind::MathBoundary { width, after } => {
                    auto_breaking = *after;
                    if auto_breaking
                        && !node.next.is_null()
                        && engine.document.nodes.get(node.next).kind.is_glue()
                    {
                        search.try_break(engine, 0, false)?;
                    }
                    search.width[0] = search.width[0].wrapping_add(width.0);
                    false
                }
                Kind::Penalty(penalty) => {
                    search.try_break(engine, *penalty, false)?;
                    false
                }
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
                        engine,
                        parameters.integer(if pre.is_empty() {
                            P::ExHyphenPenalty
                        } else {
                            P::HyphenPenalty
                        }),
                        true,
                    )?;
                    search.width[0] = search.width[0].wrapping_sub(width);
                    for _ in 0..*replace_count {
                        let node = engine.document.nodes.get(next);
                        search.width[0] = search.width[0].wrapping_add(engine.node_width(node)?);
                        next = node.next;
                    }
                    true
                }
                Kind::Insertion(_) | Kind::Adjustment(_) => true,
                Kind::Language(new) => {
                    language = *new;
                    true
                }
                Kind::Unset(_) => return Err(Error::Unsupported),
                Kind::Free => unreachable!(),
            };
            p = next;
        }
        if p.is_null() {
            search.point = NodeId::NULL;
            search.try_break(engine, -10000, true)?;
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
