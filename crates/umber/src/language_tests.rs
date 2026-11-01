use super::*;
use crate::{font_test_data, nodes::NodeId, token::cmd};

fn build(source: &[u8]) -> Engine<'_> {
    let mut e = Engine::new(source);
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    loop {
        let read = e.next_expanded().unwrap();
        if read.meaning.command == cmd::END {
            break;
        }
        e.execute(read).unwrap();
    }
    e
}
fn languages(e: &Engine<'_>, mut p: NodeId) -> Vec<Language> {
    let mut result = Vec::new();
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        if let Kind::Language(language) = node.kind {
            result.push(language);
        }
        p = node.next;
    }
    result
}
#[test]
fn paragraph_capture_precedes_everypar_and_minima_alone_do_not_emit() {
    let e = build(br"\catcode123=1\catcode125=2\font\f=test\f\language=7\lefthyphenmin=2\righthyphenmin=3\everypar{\language=8\lefthyphenmin=5}\noindent abc\lefthyphenmin=9\ abc\setlanguage8\ abc\end");
    assert_eq!(
        e.document.text.initial_language,
        Language {
            number: 7,
            left: 2,
            right: 3
        }
    );
    assert_eq!(
        languages(&e, e.document.text.horizontal.head()),
        [
            Language {
                number: 8,
                left: 5,
                right: 3
            },
            Language {
                number: 8,
                left: 9,
                right: 3
            },
        ]
    );
}
#[test]
fn language_fix_precedes_missing_nullfont_glyph_and_normalizes() {
    let e = build(br"\language=9\noindent\language=12 a\language=-1 b\language=300 c\end");
    assert_eq!(
        languages(&e, e.document.text.horizontal.head()),
        [
            Language {
                number: 12,
                left: 1,
                right: 1
            },
            Language::default(),
        ]
    );
}
#[test]
fn explicit_language_in_restricted_box_preserves_parameter_and_copy() {
    let mut e = build(br"\catcode123=1\catcode125=2\font\f=test\f\language=9\setbox0=\hbox{\language=13 a\lefthyphenmin=-1\righthyphenmin=64\setlanguage300 b\setlanguage3 c}\end");
    let id = e.document.boxes.head(0);
    let Kind::Box(b) = &e.document.nodes.get(id).kind else {
        panic!()
    };
    let data = e.document.nodes.box_data(b);
    let expected = [
        Language {
            number: 0,
            left: 1,
            right: 63,
        },
        Language {
            number: 3,
            left: 1,
            right: 63,
        },
    ];
    assert_eq!(languages(&e, data.list.head()), expected);
    assert_eq!(e.document.scalars[Target::LANGUAGE].value, 9);
    let mut scratch = crate::nodes::CopyScratch::default();
    let copied = e
        .document
        .nodes
        .copy_list(data.list.head(), &mut scratch)
        .unwrap();
    assert_eq!(languages(&e, copied.head()), expected);
    e.document
        .nodes
        .release(copied, &mut crate::nodes::ReleaseScratch::default());
}
#[test]
fn explicit_language_is_illegal_in_vertical_mode() {
    let mut e = Engine::new(br"\setlanguage3\end");
    assert_eq!(e.advance(1), Err(Error::Syntax));
}
#[test]
fn failed_explicit_language_keeps_provisional_owner_until_restore() {
    let mut e = Engine::new(br"\count0=1\relax\noindent\setlanguage\relax\end");
    while e.outcome().counts[0] != 1 {
        e.advance(1).unwrap();
    }
    let before = e.checkpoint().unwrap();
    for _ in 0..3 {
        assert_eq!(e.advance(usize::MAX), Err(Error::Number));
        assert_eq!(e.document.nodes.live_count(), 1);
        assert!(matches!(
            e.document.nodes.get(e.document.text.horizontal.head()).kind,
            Kind::Language(_)
        ));
        e.restore(&before).unwrap();
        assert_eq!(e.document.nodes.live_count(), 0);
    }
}
