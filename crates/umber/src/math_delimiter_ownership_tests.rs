//! Exercise existing arena rejection at the two right-group transfer boundaries.
use super::*;
use crate::{
    Engine,
    execute::GroupKind,
    token::{Token, cmd},
};

#[test]
fn right_boundary_and_inner_allocation_rejections_preserve_the_remaining_roots() {
    for limit in [2, 3] {
        let source=br"\catcode123=1\catcode125=2\catcode36=3\relax\setbox0=\hbox{\kern1pt}\noindent$a\left.\right.\message{UNREAD}";
        let mut e = Engine::new(source);
        e.math.as_mut().unwrap().limit = limit;
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Capacity));
        let (frames, live, group) = if limit == 2 {
            (2, 2, GroupKind::MathLeft)
        } else {
            (1, 1, GroupKind::MathShift)
        };
        assert_eq!(e.document.math_frames.len(), frames);
        assert_eq!(e.math.as_ref().unwrap().live, live);
        assert_eq!(e.current_group(), Some(group));
        let next = e.next_expanded().unwrap();
        if limit == 2 {
            assert_eq!(
                next.token,
                Token::other(b'.'),
                "right allocation precedes operand scanning"
            );
        } else {
            assert_eq!(
                next.meaning.command,
                cmd::MESSAGE,
                "right operand consumed before Inner allocation"
            );
        }
        assert_eq!(
            e.document.nodes.live_count(),
            2,
            "unrelated register box survives"
        );
        assert!(!e.can_checkpoint());
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live, 0);
        assert!(e.document.math_frames.is_empty());
        assert_eq!(e.document.nodes.live_count(), 0);
        assert!(e.can_checkpoint());
    }
}
