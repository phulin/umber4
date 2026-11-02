use super::*;
use crate::{Progress, numeric::Scaled, scalars::Target};
const BOOT: &str = r"\catcode123=1\catcode125=2\catcode35=6 ";
fn image(setup: &str) -> Vec<u8> {
    let source = format!(r"{BOOT}{setup}\dump\undefined");
    let mut e = Engine::new(source.as_bytes());
    assert_eq!(e.advance(usize::MAX), Ok(Progress::Complete), "{setup}");
    e.encode_format().unwrap()
}

#[test]
fn portable_format_relocates_aliases_definitions_tables_and_owned_token_bytes() {
    let bytes = image(
        r"\message{SETUP}\def\pair#1,#2;{#2/#1}\let\alias\pair
\def\grab#1aba{[#1]}\def\brace#{X}\long\protected\def\protect#1{#1}
\outer\def\outside{O}\catcode126=13\def~{A}\let\single=~
\toks0={token text}\toks1=\toks0\toks2={}\count23=17
\dimen0=1.5pt\skip0=2pt plus3fil minus4pt\muskip0=5mu plus6fill
\parshape=2 1pt 10pt 2pt 20pt\mathcode97=32768\delcode98=1234\lccode65=97
\everypar{unused}\edef\savedfont{\the\font}",
    );
    let job=br"\def\pair{changed}\message{\alias a,b;/\grab aaba/\brace{Y}/\single/~/\the\toks1}\message{\meaning\protect/\the\count23/\the\dimen0/\the\skip0/\the\muskip0/\the\parshape}\savedfont\end";
    let mut loaded = Engine::from_format(&bytes, job).unwrap();
    assert!(loaded.outcome().diagnostics.is_empty());
    assert_ne!(
        loaded.document.token_registers[0]
            .owner
            .as_ref()
            .unwrap()
            .id,
        u32::MAX
    );
    assert_eq!(
        loaded.document.token_registers[0]
            .owner
            .as_ref()
            .unwrap()
            .id,
        loaded.document.token_registers[1]
            .owner
            .as_ref()
            .unwrap()
            .id
    );
    assert!(loaded.document.token_registers[2].owner.is_some());
    drop(bytes);
    loaded.advance(usize::MAX).unwrap();
    let messages = loaded.outcome().message_bytes();
    assert_eq!(messages[0], b"b/a/[a]/X{Y}/A/A/token text");
    assert_eq!(messages[1],br"\protected\long macro:#1->#1/17/1.5pt/2.0pt plus 3.0fil minus 4.0pt/5.0mu plus 6.0fill/2");
}

#[test]
fn startup_hook_calendar_magnification_and_checkpoints_belong_to_the_new_job() {
    let bytes = image(
        r"\count0=7\time=999\day=8\month=9\year=1900\mag=1000\dimen0=1truept\prevdepth=5pt\everyjob{\global\advance\count0by1\message{HOOK:\the\year/\the\time}}",
    );
    let job = br"\mag=2000\dimen1=1truept\message{JOB:\the\count0/\the\dimen1}\end";
    let mut e = Engine::from_format(&bytes, job).unwrap();
    assert_eq!(e.document.scalars[Target::parameter(5)].value, 1970);
    assert_eq!(e.document.text.prev_depth, Scaled(-65_536_000));
    assert!(e.document.layout_values.prepared_mag.is_none());
    assert_eq!(e.encode_format(), Err(Error::Checkpoint));
    e.set_calendar(123, 2, 3, 2026);
    let checkpoint = e.checkpoint().unwrap();
    for _ in 0..2 {
        e.advance(usize::MAX).unwrap();
        assert_eq!(
            e.outcome().message_bytes(),
            [b"HOOK:2026/123".to_vec(), b"JOB:8/0.5pt".to_vec()]
        );
        e.restore(&checkpoint).unwrap();
    }
    let mut other = Engine::from_format(&bytes, br"\message{\the\count0}\end").unwrap();
    assert_eq!(other.restore(&checkpoint), Err(Error::Checkpoint));
    other.advance(usize::MAX).unwrap();
    assert_eq!(
        other.outcome().message_bytes(),
        [b"HOOK:1970/0".to_vec(), b"8".to_vec()]
    );
}

#[test]
fn dump_abandons_invocation_sources_but_rejects_semantic_work_and_dependent_state() {
    let source = format!(r"{BOOT}\def\done#1{{\dump#1\undefined}}\done{{retained}}");
    let mut e = Engine::new(source.as_bytes());
    e.advance(usize::MAX).unwrap();
    assert!(e.document.sources.is_empty());
    assert!(e.packets.iter().all(|p| p.owners.is_empty()));
    assert!(e.encode_format().is_ok());
    for (source, error) in [
        (r"{\dump}", Error::Group),
        (r"\iftrue\dump", Error::Conditional),
        (r"\afterassignment\relax\dump", Error::Checkpoint),
    ] {
        let source = format!("{BOOT}{source}");
        let mut e = Engine::new(source.as_bytes());
        assert_eq!(e.advance(usize::MAX), Err(error));
    }
    {
        let body = r"\setbox0=\hbox{\kern1pt}";
        let source = format!(r"{BOOT}{body}\end");
        let mut e = Engine::new(source.as_bytes());
        e.advance(usize::MAX).unwrap();
        assert_eq!(e.encode_format(), Err(Error::Unsupported), "{body}");
    }
    let source = format!(r"{BOOT}\font\f=test\end");
    let mut e = Engine::new(source.as_bytes());
    e.register_tfm(b"test", &crate::font_test_data::basic())
        .unwrap();
    e.advance(usize::MAX).unwrap();
    let image = e.encode_format().unwrap();
    let loaded = Engine::from_format(&image, b"").unwrap();
    assert_eq!(loaded.document.fonts.len, 2);
    let source = format!(r"{BOOT}\def\m{{\count0=1\count0=2}}\m\end");
    let mut e = Engine::new(source.as_bytes());
    e.advance(2).unwrap();
    assert_eq!(e.encode_format(), Err(Error::Checkpoint));
}

#[test]
fn extra_immutable_resources_are_registered_before_job_execution_and_do_not_override() {
    let bytes = image("");
    let tfm = crate::font_test_data::basic();
    let mut changed = tfm.clone();
    changed.push(0);
    let input = InputFile {
        name: b"job.tex",
        bytes: br"\font\f=test\end",
    };
    let mut e =
        Engine::with_format_inputs(&bytes, input, None, &[(b"test", &tfm), (b"test.tfm", &tfm)])
            .unwrap();
    e.advance(usize::MAX).unwrap();
    assert_eq!(e.document.fonts.len, 2);
    assert!(matches!(
        Engine::with_format_inputs(
            &bytes,
            input,
            None,
            &[(b"test", &tfm), (b"test.tfm", &changed)]
        ),
        Err(Error::Resource)
    ));
    let mut e = Engine::from_format(&bytes, br"\patterns{a1b}\end").unwrap();
    assert_eq!(e.advance(usize::MAX), Err(Error::Syntax));
    let mut e = Engine::from_format(&bytes, br"\hyphenation{a-b}\end").unwrap();
    e.advance(usize::MAX).unwrap();
}

#[test]
fn reused_definition_headers_and_first_line_startup_preserve_token_semantics() {
    let bytes = image(r"\def\m#1{#1}\let\m\relax\toks0={x}\everyjob{\endlinechar=-1 }");
    let mut e =
        Engine::from_format(&bytes, b"\\message{a\nb}\\message{\\the\\toks0}\\end").unwrap();
    e.advance(usize::MAX).unwrap();
    assert_eq!(
        e.outcome().message_bytes(),
        [b"a b".to_vec(), b"x".to_vec()]
    );
    let mut bytes = image(r"\def\hi#1{#1}");
    // Signature marker bytes are independent of the 255 separator at body-1.
    let values = sections(&bytes)[2].0;
    bytes[values + 19..values + 23].copy_from_slice(&Token(PARAM | 255).0.to_le_bytes());
    let mut e = Engine::from_format(&bytes, br"\message{\hi{Z}}\end").unwrap();
    e.advance(usize::MAX).unwrap();
    assert_eq!(e.outcome().message_bytes(), [b"Z".to_vec()]);
}

fn sections(image: &[u8]) -> Vec<(usize, usize)> {
    let mut position = 24;
    let mut out = Vec::new();
    for _ in 0..8 {
        let n = u64::from_le_bytes(image[position + 4..position + 12].try_into().unwrap()) as usize;
        out.push((position + 12, n));
        position += 12 + n;
    }
    out
}
#[test]
fn malformed_images_never_publish_invalid_ids_metadata_or_unbounded_allocations() {
    let bytes = image(r"\def\id#1{#1}\toks0={ok}");
    for n in 0..bytes.len() {
        assert!(Engine::from_format(&bytes[..n], b"").is_err());
    }
    let parts = sections(&bytes);
    let mut offsets = vec![
        0,
        8,
        12,
        16,
        24,
        28,
        parts[0].0 + 4,
        parts[1].0,
        parts[2].0 + 4,
        parts[2].0 + 6,
        parts[2].0 + 7,
        parts[2].0 + 11,
    ];
    let probe = Engine::new(b"");
    let hskip = probe.names.lookup(b"hskip").unwrap() as usize;
    offsets.push(parts[3].0 + 4 + hskip * 5 + 1);
    offsets.push(parts[4].0 + 4 + (256 + b'a' as usize) * 4);
    let dims = u32::from_le_bytes(bytes[parts[5].0..parts[5].0 + 4].try_into().unwrap()) as usize;
    offsets.push(parts[5].0 + 4 + dims * 4 + 4 + 12);
    for offset in offsets {
        let mut bad = bytes.clone();
        bad[offset] = 255;
        assert!(Engine::from_format(&bad, b"").is_err(), "offset {offset}");
    }
    let mut bad = bytes.clone();
    bad[parts[1].0] = 1;
    assert!(matches!(Engine::from_format(&bad, b""), Err(Error::Format)));
    let mut bad = bytes.clone();
    bad[parts[2].0..parts[2].0 + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Engine::from_format(&bad, b""),
        Err(Error::Capacity)
    ));
    let mut e = Engine::from_format(&bytes, br"\message{\id{still works}}\end").unwrap();
    e.advance(usize::MAX).unwrap();
    assert_eq!(e.outcome().message_bytes(), [b"still works".to_vec()]);
}
