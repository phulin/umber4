//! Live font spacing, native source selection and operation-local reads.
use super::*;
use crate::{
    nodes::GlueKind,
    numeric::{Glue, GlueOrder},
};

#[test]
fn spacing_sf_codes_and_control_spaces() {
    let cases:[(&[u8],i32,i32);4]=[
        (br"\font\f=test \f\noindent\sfcode`\.=3000 a. \end",291270,327678),
        (br"\font\f=test \f\noindent\sfcode`\.=3000 A. \end",218453,109226),
        (br"\font\f=test \f\noindent\spacefactor=2000\spaceskip=5pt plus 3pt minus 2pt\fontdimen7\f=1pt \ \end",5*65536,3*65536),
        (br"\font\f=test \f\noindent\xspaceskip=9pt\spacefactor=2000\relax{} \end",9*65536,0),
    ];
    for (source, width, stretch) in cases {
        let mut e = engine(source);
        build(&mut e);
        let mut p = e.document.text.horizontal.head();
        let mut glues = Vec::new();
        while !p.is_null() {
            let n = e.document.nodes.get(p);
            if let Kind::Glue { spec, .. } = &n.kind {
                glues.push(*spec);
            }
            p = n.next;
        }
        assert_eq!(glues.len(), 1);
        assert_eq!(glues[0].width.0, width);
        assert_eq!(glues[0].stretch.0, stretch);
    }
}

fn glues(e: &Engine<'_>) -> Vec<(Glue, GlueKind)> {
    let mut p = e.document.text.horizontal.head();
    let mut result = Vec::new();
    while !p.is_null() {
        let node = e.document.nodes.get(p);
        if let Kind::Glue { spec, subtype } = node.kind {
            result.push((spec, subtype));
        }
        p = node.next;
    }
    result
}
#[test]
fn spacing_selects_full_glue_and_preserves_source_subtypes_and_orders() {
    use GlueKind::{Normal, SpaceSkip, XSpaceSkip};
    use GlueOrder::{Fil, Fill};
    let cases = [
        (
            r"\spaceskip=0pt plus1fil minus-2fill\relax{} ",
            Glue {
                width: Scaled::ZERO,
                stretch: Scaled(65536),
                shrink: Scaled(-2 * 65536),
                stretch_order: Fil,
                shrink_order: Fill,
            },
            SpaceSkip,
        ),
        (
            r"\spaceskip=0pt plus1fil minus-2fill\spacefactor=500\relax{} ",
            Glue {
                width: Scaled::ZERO,
                stretch: Scaled(32768),
                shrink: Scaled(-4 * 65536),
                stretch_order: Fil,
                shrink_order: Fill,
            },
            Normal,
        ),
        (
            r"\xspaceskip=9pt plus2fil minus3fill\fontdimen2\f=12pt\fontdimen7\f=13pt\spacefactor=2000\relax{} ",
            Glue {
                width: Scaled(9 * 65536),
                stretch: Scaled(2 * 65536),
                shrink: Scaled(3 * 65536),
                stretch_order: Fil,
                shrink_order: Fill,
            },
            XSpaceSkip,
        ),
        (
            r"\spaceskip=5pt plus3fil minus2fill\fontdimen7\f=1pt\spacefactor=2000\relax{} ",
            Glue {
                width: Scaled(6 * 65536),
                stretch: Scaled(6 * 65536),
                shrink: Scaled(65536),
                stretch_order: Fil,
                shrink_order: Fill,
            },
            Normal,
        ),
        (
            r"\xspaceskip=9pt\fontdimen2\f=1pt\fontdimen3\f=2pt\fontdimen4\f=3pt\fontdimen7\f=4pt\spacefactor=3000\ ",
            Glue {
                width: Scaled(65536),
                stretch: Scaled(2 * 65536),
                shrink: Scaled(3 * 65536),
                ..Glue::default()
            },
            Normal,
        ),
    ];
    for (body, spec, subtype) in cases {
        let source = format!(r"\font\f=test\f\noindent{body}\end");
        let mut e = engine(source.as_bytes());
        build(&mut e);
        assert_eq!(glues(&e), [(spec, subtype)], "{body}");
    }
}
#[test]
fn spacing_reads_live_parameters_and_local_font_selection_after_restore() {
    let source = concat!(
        r"\font\f=test\font\g=test at12pt\f\noindent",
        r"\fontdimen2\f=1pt\fontdimen3\f=2pt\fontdimen4\f=3pt\fontdimen7\f=4pt",
        r"\fontdimen2\g=9pt\fontdimen3\g=10pt\fontdimen4\g=11pt\fontdimen7\g=12pt",
        r"\spacefactor=2000\relax{} ",
        r"{\g\spacefactor=2000\relax{} ",
        r"\fontdimen2\f=5pt\fontdimen3\f=-6pt\fontdimen4\f=7pt\fontdimen7\f=8pt}",
        r"\spacefactor=2000\relax{} \g\spacefactor=2000\relax{} \end"
    );
    let mut e = engine(source.as_bytes());
    let initial = e.checkpoint().unwrap();
    for _ in 0..2 {
        build(&mut e);
        let values = glues(&e);
        assert_eq!(
            values
                .iter()
                .map(|(g, _)| (g.width.0, g.stretch.0, g.shrink.0))
                .collect::<Vec<_>>(),
            [
                (5 * 65536, 4 * 65536, 3 * 32768),
                (21 * 65536, 20 * 65536, 11 * 32768),
                (13 * 65536, -12 * 65536, 7 * 32768),
                (21 * 65536, 20 * 65536, 11 * 32768)
            ]
        );
        assert!(values.iter().all(|(_, kind)| *kind == GlueKind::Normal));
        assert_eq!(e.font_parameter(1, 2), Scaled(5 * 65536));
        e.restore(&initial).unwrap();
    }
}
#[test]
fn xspaceskip_bypasses_scaling_and_failed_scaling_appends_no_glue() {
    let source=br"\font\f=test\f\noindent\fontdimen3\f=10000pt\xspaceskip=9pt\spacefactor=32767\relax{} \xspaceskip=0pt\relax{} \end";
    let mut e = engine(source);
    let initial = e.checkpoint().unwrap();
    for _ in 0..2 {
        assert_eq!(e.advance(usize::MAX), Err(Error::Overflow));
        assert_eq!(
            glues(&e),
            [(
                Glue {
                    width: Scaled(9 * 65536),
                    ..Glue::default()
                },
                GlueKind::XSpaceSkip
            )]
        );
        assert_eq!(
            e.document.text.horizontal.head(),
            e.document.text.horizontal.tail()
        );
        e.restore(&initial).unwrap();
    }
}
