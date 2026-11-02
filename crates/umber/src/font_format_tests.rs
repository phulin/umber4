use super::*;
use crate::{InputFile, Progress, font_test_data};
const BOOT: &str = r"\catcode123=1\catcode125=2\catcode35=6 ";
const SETUP: &str = r"
\fontdimen9\nullfont=3pt\hyphenchar\nullfont=-123\skewchar\nullfont=999
\font\first=test at10pt\font\second=test at12pt\font\other=other scaled700
\fontdimen8\other=5pt\fontdimen1\first=.125pt
\hyphenchar\first=33\skewchar\first=65\first\textfont2=\second
\edef\saved{\the\font}
{\font\local=unused at9pt\global\edef\localfont{\the\local}}
\fontdimen11\localfont=-2pt\font\renamed=test at10pt\let\second=\nullfont
";
fn setup<'a>(source: &'a [u8]) -> Engine<'a> {
    let mut e = Engine::new(source);
    for name in [b"test".as_slice(), b"other", b"unused", b"unloaded"] {
        e.register_tfm(name, &font_test_data::basic()).unwrap();
    }
    e
}
fn image() -> Vec<u8> {
    let text = format!(r"{BOOT}{SETUP}\dump");
    let mut e = setup(text.as_bytes());
    assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete));
    e.encode_format().unwrap()
}
fn payload(e: &Engine<'_>) -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    write(e, &mut w).unwrap();
    w.finish().unwrap()[24..].to_vec()
}
fn load_payload(bytes: &[u8]) -> Result<()> {
    let mut e = Engine::new(b"");
    let names = e.names.len();
    let mut r = Reader::new(bytes)?;
    read(&mut e, &mut r, names, &[])?;
    r.finish()
}
#[test]
fn fonts_roundtrip_ids_parameters_selectors_and_owned_metrics() {
    let image = image();
    let copy = image.clone();
    let job = br"\message{\the\font/\the\fontdimen9\nullfont/\the\hyphenchar\first/\the\textfont2}
\shipout\hbox{\localfont ffi AV\saved ffi AV}\end";
    let text = format!("{BOOT}{SETUP}{}", std::str::from_utf8(job).unwrap());
    let mut source = setup(text.as_bytes());
    source.advance(usize::MAX).unwrap();
    let mut e = Engine::from_format(&image, job).unwrap();
    drop(image);
    assert_eq!(e.document.fonts.len, 5);
    assert_eq!(e.font_catalog.resources.len(), 3); // unused host input omitted
    assert!(e.font_catalog.sealed);
    assert_eq!(e.document.fonts.current.value, 1);
    let checkpoint = e.checkpoint().unwrap();
    for _ in 0..2 {
        e.advance(usize::MAX).unwrap();
        assert_eq!(
            e.outcome().message_bytes(),
            source.outcome().message_bytes()
        );
        let a = e.outcome();
        let b = source.outcome();
        let page = a.pages.get(0).unwrap();
        let expected = b.pages.get(0).unwrap();
        assert_eq!(
            page.placements().collect::<Vec<_>>(),
            expected.placements().collect::<Vec<_>>()
        );
        assert_eq!(page.fonts().len(), expected.fonts().len());
        for (a, b) in page.fonts().iter().zip(expected.fonts()) {
            assert_eq!(
                (a.name(), a.size(), a.tfm_bytes()),
                (b.name(), b.size(), b.tfm_bytes())
            );
        }
        e.restore(&checkpoint).unwrap();
    }
    for id in 0..e.document.fonts.len {
        let a = e.document.fonts.descriptors[id];
        let b = source.document.fonts.descriptors[id];
        assert_eq!(
            (a.count, a.hyphen, a.skew, a.identifier),
            (b.count, b.hyphen, b.skew, b.identifier)
        );
        for i in 0..a.count {
            assert_eq!(
                e.document.fonts.parameters[a.params + i],
                source.document.fonts.parameters[b.params + i]
            );
        }
    }
    // Independent jobs own mutable roots while retaining their immutable metrics.
    let image = copy;
    let mut changed = Engine::from_format(&image, br"\fontdimen1\first=8pt\end").unwrap();
    changed.advance(usize::MAX).unwrap();
    assert_eq!(e.font_parameter(1, 1), Scaled(8192));
    assert_eq!(changed.font_parameter(1, 1), Scaled(8 * 65536));
}

#[test]
fn fonts_overlays_merge_before_sealing_and_conflicts_never_override() {
    let image = image();
    let tfm = font_test_data::basic();
    let mut different = tfm.clone();
    different.push(0);
    let input = InputFile {
        name: b"job",
        bytes: br"\font\extra=extra\end",
    };
    let mut e = Engine::with_format_inputs(
        &image,
        input,
        None,
        &[(b"test.tfm", &tfm), (b"extra", &tfm)],
    )
    .unwrap();
    e.advance(usize::MAX).unwrap();
    assert_eq!(e.document.fonts.len, 6);
    assert_eq!(e.font_catalog.resources.len(), 4);
    assert!(matches!(
        Engine::with_format_inputs(&image, input, None, &[(b"test", &different)]),
        Err(Error::Resource)
    ));
}

#[test]
fn nullfont_changes_use_full_marker_and_survive_readback() {
    let text =
        format!(r"{BOOT}\fontdimen10\nullfont=-3pt\hyphenchar\nullfont=1\skewchar\nullfont=2\dump");
    let mut e = Engine::new(text.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert_eq!(payload(&e)[0], 1);
    let image = e.encode_format().unwrap();
    let loaded = Engine::from_format(&image, b"").unwrap();
    let f = loaded.document.fonts.descriptors[0];
    assert_eq!((f.count, f.hyphen, f.skew), (10, 1, 2));
    assert_eq!(loaded.font_parameter(0, 10), Scaled(-3 * 65536));
    assert!(!loaded.font_catalog.sealed);
    let fresh = Engine::new(b"");
    assert_eq!(payload(&fresh), [0]);
    assert_eq!(load_payload(&[0]), Ok(()));
    assert_eq!(load_payload(&[0, 1]), Err(Error::Format));
}

fn null_record() -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    w.u8(1);
    w.count(0).unwrap();
    w.count(1).unwrap();
    w.i32(0);
    w.u32(0);
    w.i32(0);
    w.count(7).unwrap();
    w.i32(45);
    w.i32(-1);
    w.u32(Engine::new(b"").names.lookup(b"nullfont").unwrap());
    for _ in 0..7 {
        w.i32(0);
    }
    w.finish().unwrap()[24..].to_vec()
}
#[test]
fn font_wire_checks_truncation_counts_ids_sizes_and_raw_resources() {
    let valid = null_record();
    assert_eq!(load_payload(&valid), Ok(()));
    for n in 0..valid.len() {
        assert!(load_payload(&valid[..n]).is_err(), "{n}");
    }
    for (offset, value) in [
        (1, u32::MAX),
        (5, 0),
        (9, 1),
        (13, 1),
        (17, 1),
        (21, 6),
        (33, u32::MAX),
    ] {
        let mut bad = valid.clone();
        bad[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(load_payload(&bad).is_err(), "{offset}");
    }
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    w.u8(1);
    w.count(1).unwrap();
    w.blob(b"bad").unwrap();
    w.blob(&[0; 24]).unwrap();
    assert_eq!(load_payload(&w.finish().unwrap()[24..]), Err(Error::Format));
    // Noncanonical duplicate embedded names and out-of-range scaled references.
    let tfm = font_test_data::basic();
    for names in [[b"test".as_slice(), b"test"], [b"test.tfm", b"other"]] {
        let mut w = Writer::default();
        w.bytes(&[0; 24]);
        w.u8(1);
        w.count(2).unwrap();
        for name in names {
            w.blob(name).unwrap();
            w.blob(&tfm).unwrap();
        }
        assert_eq!(load_payload(&w.finish().unwrap()[24..]), Err(Error::Format));
    }
}

#[test]
fn stored_resource_names_are_not_normalized_twice() {
    let text = format!(r"{BOOT}\font\f=test.tfm.tfm\dump");
    let mut e = Engine::new(text.as_bytes());
    let tfm = font_test_data::basic();
    e.register_tfm(b"test.tfm.tfm", &tfm).unwrap();
    assert_eq!(e.register_tfm(b"test.tfm.tfm", &tfm), Err(Error::Resource));
    e.register_tfm(b"test", &tfm).unwrap();
    e.advance(usize::MAX).unwrap();
    let image = e.encode_format().unwrap();
    let mut loaded = Engine::with_format_inputs(
        &image,
        InputFile {
            name: b"",
            bytes: br"\font\g=test.tfm.tfm at12pt\end",
        },
        None,
        &[(b"test.tfm.tfm", &tfm)],
    )
    .unwrap();
    loaded.advance(usize::MAX).unwrap();
    assert_eq!(loaded.font_catalog.resources.len(), 1);
    assert_eq!(loaded.font_catalog.scaled[0].name(), b"test.tfm");
    assert_eq!(loaded.font_catalog.scaled[1].name(), b"test.tfm");
}

#[test]
fn scaled_cache_and_search_budgets_precede_derived_allocations() {
    let mut catalog = Catalog::default();
    let mut tfm = font_test_data::tfm_range(0, 0, &[], &[]);
    let kern_start = tfm.len() - 8 * 4;
    tfm.splice(kern_start..kern_start, std::iter::repeat_n(0, 15999 * 4));
    let words = (tfm.len() / 4) as u16;
    tfm[..2].copy_from_slice(&words.to_be_bytes());
    tfm[18..20].copy_from_slice(&16000u16.to_be_bytes());
    catalog.register(b"large", &tfm).unwrap();
    let per = catalog.scaled_allocation_bytes(0);
    assert!(per >= 16000 * 4);
    let mut budget = Budget::default();
    for i in 0..MAX_BYTES / per {
        budget.add(&catalog, 0, i).unwrap();
    }
    assert_eq!(budget.add(&catalog, 0, 0), Err(Error::Capacity));
    assert!(catalog.scaled.is_empty());
    let mut budget = Budget {
        search: MAX_BYTES,
        bytes: 0,
    };
    assert_eq!(budget.add(&catalog, 0, 1), Err(Error::Capacity));
    let mut budget = Budget {
        search: 0,
        bytes: MAX_BYTES,
    };
    assert_eq!(budget.resources(1), Err(Error::Capacity));
    // Reject amplification before parsing even the first resource record.
    let mut w = Writer::default();
    w.bytes(&[0; 24]);
    w.u8(1);
    w.count(8193).unwrap();
    w.bytes(&vec![0; 8193 * 8]);
    assert_eq!(
        load_payload(&w.finish().unwrap()[24..]),
        Err(Error::Capacity)
    );
}

#[test]
fn font_resource_references_and_scaled_sizes_are_checked_before_publication() {
    let mut e = Engine::new(b"");
    e.register_tfm(b"test", &font_test_data::basic()).unwrap();
    let scaled = e.font_catalog.scaled(0, Scaled(10 * 65536)).unwrap();
    e.document.fonts.descriptors[1] = Font {
        scaled: Some(scaled),
        params: 7,
        count: 7,
        hyphen: 45,
        skew: -1,
        identifier: e.document.fonts.descriptors[0].identifier,
    };
    e.document.fonts.len = 2;
    e.document.fonts.parameter_end = 14;
    let valid = payload(&e);
    assert_eq!(load_payload(&valid), Ok(()));
    let mut r = Reader::new(&valid).unwrap();
    r.u8().unwrap();
    assert_eq!(r.u32().unwrap(), 1);
    r.blob().unwrap();
    r.blob().unwrap();
    let second = valid.len() - r.remaining() + 8 + 52;
    for (offset, value) in [
        (0, 0),
        (0, 2),
        (4, 0),
        (4, u32::MAX),
        (4, 134217728),
        (8, 6),
        (20, u32::MAX),
    ] {
        let mut bad = valid.clone();
        bad[second + offset..second + offset + 4].copy_from_slice(&value.to_le_bytes());
        assert_eq!(load_payload(&bad), Err(Error::Format), "{offset}/{value}");
    }
    for length in [second, second + 24, valid.len() - 1] {
        assert!(load_payload(&valid[..length]).is_err());
    }
}
