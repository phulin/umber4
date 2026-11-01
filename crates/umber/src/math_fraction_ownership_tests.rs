//! Existing arena bounds exercise fraction capture and final list handoffs.
use super::*;
use crate::{
    Engine,
    execute::GroupKind,
    token::{Token, cmd},
};

#[test]
fn fraction_capacity_rejections_release_only_transferred_box_forests() {
    let prefix = r"\catcode123=1\catcode125=2\catcode36=3\relax\setbox0=\hbox{\kern1pt}\noindent$";
    for (limit, body, frames, live, nodes, group, next) in [
        (
            1,
            r"\hbox{\kern2pt}\over\message{UNREAD}",
            1,
            1,
            4,
            GroupKind::MathShift,
            cmd::MESSAGE,
        ),
        (
            2,
            r"\hbox{\kern2pt}\over\hbox{\kern3pt}\message{UNREAD}",
            1,
            2,
            4,
            GroupKind::MathShift,
            cmd::MESSAGE,
        ),
        (
            4,
            r"a\left.\hbox{\kern2pt}\over\hbox{\kern3pt}\message{UNREAD}",
            2,
            4,
            4,
            GroupKind::MathLeft,
            cmd::MESSAGE,
        ),
        (
            5,
            r"a\left.\hbox{\kern2pt}\over\hbox{\kern3pt}\right.\message{UNREAD}",
            2,
            5,
            6,
            GroupKind::MathLeft,
            12,
        ),
        (
            6,
            r"a\left.\hbox{\kern2pt}\over\hbox{\kern3pt}\right.\message{UNREAD}",
            1,
            1,
            2,
            GroupKind::MathShift,
            cmd::MESSAGE,
        ),
    ] {
        let source = format!("{prefix}{body}");
        let mut e = Engine::new(source.as_bytes());
        e.math.as_mut().unwrap().limit = limit;
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Capacity), "limit{limit}");
        assert_eq!(e.document.math_frames.len(), frames);
        assert_eq!(e.math.as_ref().unwrap().live, live);
        assert_eq!(e.document.nodes.live_count(), nodes);
        assert_eq!(e.current_group(), Some(group));
        if limit == 1 {
            let frame = e.document.math_frames.last().unwrap();
            assert!(frame.incomplete.is_none());
            assert!(
                !frame.list.is_empty(),
                "original boxed numerator remains current"
            );
        }
        assert_eq!(e.next_expanded().unwrap().meaning.command, next);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn completed_left_fraction_owns_each_boundary_and_box_once() {
    let mut e=Engine::new(br"\catcode123=1\catcode125=2\catcode36=3\relax\noindent$a\left.\hbox{\kern2pt}\over\hbox{\kern3pt}\right.\relax");
    let checkpoint = e.checkpoint().unwrap();
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX && e.math.as_ref().unwrap().live == 7 {
            break;
        }
        e.execute(read).unwrap();
    }
    let math = e.math.as_ref().unwrap();
    let inner = e.document.math_frames.last().unwrap().list.tail().unwrap();
    let Kind::Atom(atom) = &math.node(inner).kind else {
        unreachable!()
    };
    assert_eq!(atom.class, Class::Inner);
    let Field::SubMath(list) = &atom.fields.nucleus else {
        unreachable!()
    };
    let left = list.head;
    assert!(matches!(
        math.node(left).kind,
        Kind::Boundary {
            side: Side::Left,
            ..
        }
    ));
    let fraction = math.node(left).next;
    let Kind::Fraction(data) = &math.node(fraction).kind else {
        unreachable!()
    };
    assert!(matches!(math.node(data.numerator.head).kind, Kind::Atom(_)));
    assert_eq!(data.numerator.head, data.numerator.tail);
    assert!(matches!(
        math.node(data.denominator.head).kind,
        Kind::Atom(_)
    ));
    assert_eq!(data.denominator.head, data.denominator.tail);
    let right = math.node(fraction).next;
    assert_eq!(right, list.tail);
    assert!(matches!(
        math.node(right).kind,
        Kind::Boundary {
            side: Side::Right,
            ..
        }
    ));
    assert_eq!(math.node(right).next, MathId::NULL);
    let frame = e.document.math_frames.pop().unwrap();
    assert!(frame.incomplete.is_none());
    e.math
        .as_mut()
        .unwrap()
        .release(frame.list, &mut e.document.nodes);
    assert_eq!(e.math.as_ref().unwrap().live, 0);
    assert_eq!(e.document.nodes.live_count(), 0);
    e.restore(&checkpoint).unwrap();
    assert_eq!(
        e.next_expanded().unwrap().token,
        Token::cs(e.names.lookup(b"catcode").unwrap())
    );
}
