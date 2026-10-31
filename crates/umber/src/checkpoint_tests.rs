use super::*;

const BOOT: &str = "\\catcode123=1 \\catcode125=2 \\catcode35=6\\relax\n";

fn finish(engine: &mut Engine<'_>) -> Outcome {
    while engine.advance(7).unwrap() != Progress::Complete {}
    engine.outcome()
}
fn same(a: &Outcome, b: &Outcome) {
    assert_eq!(a.counts, b.counts);
    assert_eq!(a.messages, b.messages);
}

/// Every successful command boundary, including those inside a live invocation,
/// is exercised twice. A snapshot remains immutable across repeated branches.
fn replay_boundaries(source: &[u8]) {
    let expected = Engine::new(source).run().unwrap();
    let mut engine = Engine::new(source);
    loop {
        let snapshot = engine.checkpoint().unwrap();
        let prefix = engine.outcome();
        for _ in 0..2 {
            same(&finish(&mut engine), &expected);
            engine.restore(&snapshot).unwrap();
            same(&engine.outcome(), &prefix);
        }
        if engine.advance(1).unwrap() == Progress::Complete {
            break;
        }
    }
    let completed = engine.checkpoint().unwrap();
    assert_eq!(engine.advance(0).unwrap(), Progress::Complete);
    engine.restore(&completed).unwrap();
    assert_eq!(engine.advance(1).unwrap(), Progress::Complete);
    same(&engine.outcome(), &expected);
}

#[test]
fn assignments_groups_conditionals_and_deferred_tokens_replay() {
    let source = format!(
        r"{BOOT}
\def\old{{OLD}}\let\alias=\old\count0=1\toks0={{OLD}}
\def\groupa{{\count2=1 }}\def\groupb{{\advance\count2by2 }}
{{\def\old{{LOCAL}}\count0=2\toks0={{LOCAL}}
\aftergroup\groupa\aftergroup\groupb
\global\count0=3\count0=4
\global\toks0={{GLOBAL}}\toks0={{LAST}}
\global\def\old{{GLOBAL}}\def\old{{LAST}}
\iftrue\message{{\old/\the\toks0/\the\count0}}\else\undefined\fi
\let\alias=\relax}}
\message{{\old/\the\toks0/\the\count0}}
\def\after{{\advance\count1 by1 }}\afterassignment\after\count1=8
\begingroup\aftergroup\after\count1=12\endgroup
\futurelet\seen\relax\old
\message{{\the\count1/\meaning\seen}}\end"
    );
    // The final ordinary \old would typeset text; replace its original token
    // with a no-op alias before the futurelet continuation reads it.
    let source = source.replace(
        r"\futurelet\seen\relax\old",
        r"\let\old=\relax\futurelet\seen\relax\old",
    );
    replay_boundaries(source.as_bytes());
    let outcome = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(outcome.counts[0], 3);
    assert_eq!(outcome.counts[1], 10);
    assert_eq!(outcome.counts[2], 3);
    assert_eq!(
        outcome.messages,
        [b"LAST/LAST/4".as_slice(), b"GLOBAL/GLOBAL/3", b"10/\\relax"]
    );
}

#[test]
fn active_forwarded_arguments_and_replaced_definitions_replay() {
    let source = format!(
        r"{BOOT}
\def\inner#1{{#1\message{{INNER}}#1}}
\def\outer#1{{\def\outer{{REPLACED}}\inner{{#1}}#1}}
\outer{{\message{{ARG}}\advance\count0by1\relax}}
\toks0={{\message{{TOKS}}\count1=9\relax}}
\the\toks0\message{{\the\count0/\the\count1}}\end"
    );
    replay_boundaries(source.as_bytes());
    let outcome = Engine::new(source.as_bytes()).run().unwrap();
    assert_eq!(outcome.counts[..2], [3, 9]);
    assert_eq!(
        outcome.messages,
        [b"ARG".as_slice(), b"INNER", b"ARG", b"ARG", b"TOKS", b"3/9"]
    );
}

#[test]
fn fragmented_arguments_and_one_read_suppression_replay() {
    let source = format!(
        r"{BOOT}
\def\bad{{\message{{BAD}}}}
\expandafter\relax\noexpand\bad
\def\take#1{{#1#1}}
\def\join#1#2{{\take{{#1\message{{MID}}#2}}}}
\join{{\message{{LEFT}}}}{{\message{{RIGHT}}}}
\message{{OK}}\end"
    );
    replay_boundaries(source.as_bytes());
    assert_eq!(
        Engine::new(source.as_bytes()).run().unwrap().messages,
        [
            b"LEFT".as_slice(),
            b"MID",
            b"RIGHT",
            b"LEFT",
            b"MID",
            b"RIGHT",
            b"OK"
        ]
    );
}

#[test]
fn selected_case_else_and_nested_conditional_phases_replay() {
    let source = format!(
        r"{BOOT}
\iffalse\undefined\else
  \message{{ELSE}}
  \ifcase1 \undefined\or\message{{ONE}}\or\undefined\else\undefined\fi
\fi
\ifcase9 \undefined\or\undefined\else
  \iftrue\message{{CASE-ELSE}}\else\undefined\fi
\fi\end"
    );
    replay_boundaries(source.as_bytes());
    assert_eq!(
        Engine::new(source.as_bytes()).run().unwrap().messages,
        [b"ELSE".as_slice(), b"ONE", b"CASE-ELSE"]
    );
}

#[test]
fn snapshot_save_owners_outlive_the_registry() {
    let source = format!(r"{BOOT}\def\value{{OLD}}{{\def\value{{NEW}}\relax}}\end");
    let mut engine = Engine::new(source.as_bytes());
    while !engine
        .document
        .save
        .iter()
        .any(|s| matches!(s, Save::Meaning(_, _, Some(_))))
    {
        engine.advance(1).unwrap();
    }
    let snapshot = engine.checkpoint().unwrap();
    drop(engine);
    for saved in snapshot.save.iter() {
        if let Save::Meaning(_, _, Some(owner)) = saved {
            let definition = owner.definition();
            assert_eq!(
                owner.tokens.get(definition.body.start),
                Token::character(11, b'O')
            );
        }
    }
}

#[test]
fn prepared_lines_categories_and_committed_translations_replay() {
    let source = format!(
        r"{BOOT}
\catcode94=7\relax
\def\name{{YES}}\message{{\na^^6de}}
\catcode64=11\relax\def\with@name{{AT}}\message{{\with@name}}
{{\catcode33=13\relax\def!{{ACTIVE}}\message{{!}}}}
\endlinechar=33\relax
\message{{LINE
NEXT}}\end"
    );
    replay_boundaries(source.as_bytes());
}

#[test]
fn names_survive_rollback_but_their_meanings_do_not() {
    let source = format!(r"{BOOT}\def\newname{{NEW}}\message{{\newname}}\end");
    let mut engine = Engine::new(source.as_bytes());
    let start = engine.checkpoint().unwrap();
    let expected = finish(&mut engine);
    let id = engine.intern(b"newname").unwrap();
    let slots = engine.names.len();
    assert!(engine.resolve(Token::cs(id)).is_macro());
    engine.restore(&start).unwrap();
    assert_eq!(engine.names.len(), slots);
    assert_eq!(engine.intern(b"newname").unwrap(), id);
    assert_eq!(engine.resolve(Token::cs(id)), Meaning::UNDEFINED);
    same(&finish(&mut engine), &expected);
}

#[test]
fn abandoned_failed_scans_can_restore_without_retaining_packet_values() {
    let source = format!(r"{BOOT}\def\take#1#2{{#1#2}}\take{{captured}}");
    let mut engine = Engine::new(source.as_bytes());
    let start = engine.checkpoint().unwrap();
    for _ in 0..3 {
        assert!(engine.advance(100).is_err());
        assert!(matches!(engine.checkpoint(), Err(Error::Checkpoint)));
        assert_eq!(engine.advance(0), Err(Error::Checkpoint));
        engine.restore(&start).unwrap();
        assert!(engine.packets.iter().all(|p| p.owners.is_empty()));
        assert_eq!(engine.values.retained().0, 0);
    }
}

#[test]
fn foreign_sessions_are_rejected_before_mutating_the_live_branch() {
    let mut engine = Engine::new(br"\count0=9\relax\end");
    engine.advance(1).unwrap();
    let other = Engine::new(br"\end").checkpoint().unwrap();
    let before = engine.outcome();
    assert_eq!(engine.restore(&other), Err(Error::Checkpoint));
    same(&before, &engine.outcome());
    assert_eq!(finish(&mut engine).counts[0], 9);
}

#[test]
fn multi_window_argument_snapshots_survive_both_drop_orders() {
    let body = r"\relax".repeat(4500);
    let source = format!(r"{BOOT}\def\take#1{{#1#1}}\take{{{body}}}\message{{DONE}}\end");
    for checkpoint_first in [false, true] {
        let mut engine = Engine::new(source.as_bytes());
        // Find a boundary within the installed argument, beyond one page.
        while !engine.document.sources.iter().any(|s| {
            s.cursor()
                .is_some_and(|c| c.argument.is_some() && c.end_position > 4096)
        }) {
            assert_ne!(engine.advance(1).unwrap(), Progress::Complete);
        }
        let retained = engine.checkpoint().unwrap();
        let token_count: usize = retained
            .packets
            .iter()
            .flat_map(|p| p.owners.iter())
            .map(|o| o.tokens.len as usize)
            .sum();
        assert!(token_count >= 4500);
        same(
            &finish(&mut engine),
            &Engine::new(source.as_bytes()).run().unwrap(),
        );
        engine.restore(&retained).unwrap();
        assert_eq!(finish(&mut engine).messages, [b"DONE"]);
        if checkpoint_first {
            drop(retained);
            drop(engine);
        } else {
            drop(engine);
            // The value registry has gone; copied descriptors still retain
            // every token page through the snapshot's slab/metadata owners.
            for owner in retained.packets.iter().flat_map(|p| p.owners.iter()) {
                assert!(owner.tokens.len > 0);
                let _ = owner.tokens.get(owner.tokens.len - 1);
            }
            drop(retained);
        }
    }
}

#[test]
fn retained_older_and_younger_roots_release_abandoned_branches() {
    let source = format!(
        r"{BOOT}\def\value{{ONE}}\message{{\value}}\def\value{{TWO}}\message{{\value}}\end"
    );
    let mut engine = Engine::new(source.as_bytes());
    let old = engine.checkpoint().unwrap();
    while engine.outcome().messages.is_empty() {
        engine.advance(1).unwrap();
    }
    let young = engine.checkpoint().unwrap();
    let prefix = engine.outcome();
    let expected = finish(&mut engine);
    engine.restore(&old).unwrap();
    drop(old);
    engine.restore(&young).unwrap();
    same(&prefix, &engine.outcome());
    same(&finish(&mut engine), &expected);
    drop(young);
}

#[test]
fn zero_budget_and_initialization_roots_do_not_consume_input() {
    let mut engine = Engine::new(br"\count0=17\relax\end");
    engine.set_calendar(42, 2, 3, 2000);
    assert_eq!(engine.advance(0), Ok(Progress::Boundary));
    let initial = engine.checkpoint().unwrap();
    assert_eq!(engine.outcome().counts[0], 0);
    finish(&mut engine);
    engine.set_calendar(0, 1, 1, 1970);
    engine.restore(&initial).unwrap();
    assert_eq!(
        engine.document.scalars[crate::scalars::Target::parameter(2)].value,
        42
    );
    assert_eq!(finish(&mut engine).counts[0], 17);
}

#[test]
fn dimension_glue_groups_magnification_and_pending_input_replay() {
    replay_boundaries(include_bytes!("../../../fixtures/mvp/layout-values.tex"));
}

#[test]
fn node_ids_resolve_in_the_captured_store_version() {
    use crate::nodes::{Kind, List, ReleaseScratch};
    let mut engine = Engine::new(br"\end");
    let node = engine.document.nodes.alloc(Kind::Penalty(7)).unwrap();
    let id = node.id();
    let mut chain = List::default();
    engine.document.nodes.append(&mut chain, node);
    // The following slices will keep this corresponding list root in Document.
    let retained_chain = chain.snapshot();
    let saved = engine.checkpoint().unwrap();
    engine.document.nodes.get_mut(id).kind = Kind::Penalty(99);
    engine
        .document
        .nodes
        .release(chain, &mut ReleaseScratch::default());
    engine.restore(&saved).unwrap();
    assert!(matches!(
        engine.document.nodes.get(retained_chain.head()).kind,
        Kind::Penalty(7)
    ));
    engine.restore(&saved).unwrap();
    assert!(matches!(
        engine.document.nodes.get(id).kind,
        Kind::Penalty(7)
    ));
}
