//! Bound the existing arena at branch/list handoffs, without production hooks.
use super::*;
use crate::{Engine, execute::GroupKind, token::cmd};

#[test]
fn choice_capacity_failures_keep_completed_and_active_forests_rooted() {
    let boot = r"\catcode123=1\catcode125=2\catcode36=3\setbox0=\hbox{\kern1pt}\noindent$a";
    for (limit, body, group, next, live, nodes) in [
        (
            1,
            r"\mathchoice{}{}{}{}",
            GroupKind::MathShift,
            cmd::BEGIN,
            1,
            2,
        ),
        (
            2,
            r"\mathchoice{\hbox{\kern2pt}\message{UNREAD}}{}{}{}",
            GroupKind::MathChoice,
            cmd::MESSAGE,
            2,
            2,
        ),
        (
            4,
            r"\mathchoice{\hbox{\kern2pt}}{\hbox{\kern3pt}}{\hbox{\kern4pt}\message{UNREAD}}{}",
            GroupKind::MathChoice,
            cmd::MESSAGE,
            4,
            6,
        ),
        (
            4,
            r"\mathchoice{\hbox{\kern2pt}\over\hbox{\kern3pt}\message{UNREAD}}{}{}{}",
            GroupKind::MathChoice,
            cmd::MESSAGE,
            4,
            4,
        ),
    ] {
        let source = format!("{boot}{body}");
        let mut e = Engine::new(source.as_bytes());
        e.math.as_mut().unwrap().limit = limit;
        let checkpoint = e.checkpoint().unwrap();
        for _ in 0..3 {
            assert_eq!(e.advance(usize::MAX), Err(Error::Capacity), "{body}");
            assert_eq!(e.current_group(), Some(group));
            assert_eq!(e.math.as_ref().unwrap().live, live);
            assert_eq!(e.document.nodes.live_count(), nodes);
            assert_eq!(e.next_expanded().unwrap().meaning.command, next);
            e.restore(&checkpoint).unwrap();
            assert_eq!(e.math.as_ref().unwrap().live, 0);
            assert_eq!(e.document.nodes.live_count(), 0);
        }
    }
}

#[test]
fn choice_branch_finalizer_transfers_fraction_and_boxes_exactly_once() {
    let source=br"\catcode123=1\catcode125=2\catcode36=3\catcode94=7\setbox0=\hbox{\kern1pt}\noindent$a\mathchoice{\hbox{\kern2pt}\over\hbox{\kern3pt}}{\hbox{\kern4pt}}{}{\hbox{\kern5pt}}^b\relax";
    let mut e = Engine::new(source);
    let checkpoint = e.checkpoint().unwrap();
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX && e.document.math_frames.len() == 1 {
            break;
        }
        e.execute(read).unwrap();
    }
    let math = e.math.as_ref().unwrap();
    let frame = e.document.math_frames.last().unwrap();
    let choice = math.node(frame.list.head).next;
    assert!(!math.scripts_allowed(choice));
    let Kind::Choice(branches) = &math.node(choice).kind else {
        unreachable!()
    };
    let Kind::Fraction(f) = &math.node(branches[0].head).kind else {
        unreachable!()
    };
    assert!(!f.numerator.is_empty() && !f.denominator.is_empty());
    assert!(branches[2].is_empty());
    let Kind::Atom(script) = &math.node(frame.list.tail).kind else {
        unreachable!()
    };
    assert!(script.fields.nucleus.is_empty());
    assert!(!script.fields.sup.is_empty());
    assert_eq!(math.live, 8);
    assert_eq!(e.document.nodes.live_count(), 10);
    let frame = e.document.math_frames.pop().unwrap();
    e.math
        .as_mut()
        .unwrap()
        .release(frame.list, &mut e.document.nodes);
    assert_eq!(e.math.as_ref().unwrap().live, 0);
    assert_eq!(e.document.nodes.live_count(), 2);
    e.restore(&checkpoint).unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
}
