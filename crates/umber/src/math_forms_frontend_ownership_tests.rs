use super::*;
use crate::{
    Engine,
    execute::GroupKind,
    token::{Token, cmd},
};
const BOOT: &str = r"\catcode123=1\catcode125=2\catcode36=3\catcode95=8\relax";

#[test]
fn rooted_form_and_vcenter_allocation_rejections_preserve_unrelated_roots() {
    for body in [
        r"\overline\message{UNREAD}",
        r"\radical\message{UNREAD}",
        r"\mathaccent\message{UNREAD}",
        r"\vcenter{\hbox{\kern2pt}}\message{UNREAD}",
    ] {
        let source = format!(r"{BOOT}\setbox0=\hbox{{\kern1pt}}\noindent$a{body}");
        let mut e = Engine::new(source.as_bytes());
        e.math.as_mut().unwrap().limit = 1;
        let checkpoint = e.checkpoint().unwrap();
        assert_eq!(e.advance(usize::MAX), Err(Error::Capacity), "{body}");
        assert_eq!(e.math.as_ref().unwrap().live, 1);
        assert_eq!(e.document.nodes.live_count(), 2);
        assert!(e.document.box_frames.is_empty());
        assert_eq!(e.current_group(), Some(GroupKind::MathShift));
        assert_eq!(e.next_expanded().unwrap().meaning.command, cmd::MESSAGE);
        e.restore(&checkpoint).unwrap();
        assert_eq!(e.math.as_ref().unwrap().live, 0);
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}

#[test]
fn accent_folding_moves_all_owned_fields_into_the_stable_parent_cell() {
    let source = format!(
        "{BOOT}{}",
        r"\catcode94=7\setbox0=\hbox{\kern1pt}\noindent$a{\mathaccent94 {\hbox{\kern2pt}}_{\hbox{\kern3pt}}}\relax"
    );
    let mut e = Engine::new(source.as_bytes());
    e.math.as_mut().unwrap().limit = 4;
    let checkpoint = e.checkpoint().unwrap();
    let mut parent = None;
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX
            && parent.is_some()
            && e.document.math_frames.len() == 1
        {
            break;
        }
        e.execute(read).unwrap();
        if parent.is_none() && e.document.math_frames.len() == 2 {
            parent = e.document.math_frames[0].list.tail();
        }
    }
    let parent = parent.unwrap();
    let frame = e.document.math_frames.last().unwrap();
    assert_eq!(frame.list.tail(), Some(parent));
    let math = e.math.as_ref().unwrap();
    assert_eq!(math.live, 2);
    let Kind::Accent { fields, .. } = &math.node(parent).kind else {
        unreachable!()
    };
    assert!(matches!(fields.nucleus, Field::Box(_)));
    assert!(matches!(fields.sub, Field::Box(_)));
    assert!(fields.sup.is_empty());
    assert_eq!(math.node(parent).next, MathId::NULL);
    assert_eq!(e.document.nodes.live_count(), 6);
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

#[test]
fn vcenter_to_size_keeps_native_max_dimen_depth_instead_of_boxmaxdepth() {
    let source = format!(
        r"{BOOT}\boxmaxdepth=-1pt\noindent$\vcenter to10pt{{\hbox{{\vrule width1pt height1pt depth8pt}}}}\relax"
    );
    let mut e = Engine::new(source.as_bytes());
    let checkpoint = e.checkpoint().unwrap();
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::RELAX && e.math.as_ref().unwrap().live == 1 {
            break;
        }
        e.execute(read).unwrap();
    }
    let id = e.document.math_frames.last().unwrap().list.tail().unwrap();
    let Kind::VCenter(fields) = &e.math.as_ref().unwrap().node(id).kind else {
        unreachable!()
    };
    let Field::Box(b) = &fields.nucleus else {
        unreachable!()
    };
    let box_kind = &e.document.nodes.get(b.id()).kind;
    let crate::nodes::Kind::Box(id) = box_kind else {
        unreachable!()
    };
    let b = e.document.nodes.box_data(id);
    assert!(b.vertical);
    assert_eq!(b.height.0, 10 * 65536);
    assert_eq!(b.depth.0, 8 * 65536);
    assert!(e.document.box_frames.is_empty());
    e.restore(&checkpoint).unwrap();
    assert_eq!(e.document.nodes.live_count(), 0);
    assert_eq!(
        e.next_expanded().unwrap().token,
        Token::cs(e.names.lookup(b"catcode").unwrap())
    );
}
