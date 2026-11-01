use super::*;
use crate::font_test_data::{basic, tfm};
#[test]
fn checked_tables_scaling_and_signed_slant() {
    let bytes = basic();
    let mut c = Catalog::default();
    c.register(b"test", &bytes).unwrap();
    let s = c.scaled(0, Scaled(655360)).unwrap();
    let m = c.metrics(s, b'a').unwrap();
    assert_eq!(m.width.0, 491520);
    assert_eq!(m.height.0, 458751);
    assert_eq!(m.depth.0, 131071);
    assert_eq!(m.italic.0, 65536);
    assert_eq!(c.scaled[s].descriptor().params[0].0, 6553);
    assert_eq!(c.scaled[s].descriptor().kern(0).0, -65537);
    assert!(c.metrics(s, 0).is_none());
    assert_eq!(c.scaled(0, Scaled(655360)).unwrap(), s);
    let large = c.scaled(0, Scaled(13_107_201)).unwrap();
    assert_eq!(c.metrics(large, b'a').unwrap().width.0, 9_830_400);
    let mut trailing = bytes.clone();
    trailing.extend_from_slice(&[1, 2, 3]);
    let mut other = Catalog::default();
    other.register(b"trailing", &trailing).unwrap();
    assert_eq!(c.register(b"late", &trailing), Err(Error::Resource));
}
#[test]
fn malformed_ranges_cycles_programs_and_recipes() {
    for n in [0, 23, 31, 100] {
        assert!(Catalog::default().register(b"bad", &basic()[..n]).is_err());
    }
    let mut bad = basic();
    bad[3] = 1;
    assert!(Catalog::default().register(b"bad", &bad).is_err());
    let bad = tfm(&[[128, b'i', 0, 0]], &[(b'f', 1, 0)]);
    assert!(Catalog::default().register(b"bad", &bad).is_err());
    let bad = tfm(&[[0, b'i', 128, 1]], &[(b'f', 1, 0)]);
    assert!(Catalog::default().register(b"bad", &bad).is_err());
    let bad = tfm(&[], &[(b'f', 2, b'g'), (b'g', 2, b'f')]);
    assert!(Catalog::default().register(b"bad", &bad).is_err());
    let bad = tfm(&[], &[(b'f', 3, 0)]);
    assert!(Catalog::default().register(b"bad", &bad).is_err());
}
#[test]
fn staged_scaling_matches_web_not_single_multiply() {
    assert_eq!(
        scale([0, 127, 255, 255], Scaled(134_217_727)).unwrap().0,
        1_073_741_568
    );
    assert_eq!(scale([255, 240, 0, 1], Scaled(655360)).unwrap().0, -655360);
    assert_eq!(scale([0, 0, 0, 1], Scaled(655360)).unwrap().0, 0);
    assert_eq!(ratio(-7, 2, 3).unwrap(), -4);
}
#[test]
fn signed_slant_valid_extensible_and_scaled_zero_entries() {
    let mut b = basic();
    let n = b.len();
    b[n - 28..n - 24].copy_from_slice(&(-104858i32).to_be_bytes());
    let mut c = Catalog::default();
    c.register(b"negative", &b).unwrap();
    let s = c.scaled(0, Scaled(655360)).unwrap();
    assert_eq!(c.scaled[s].descriptor().params[0].0, -6554);
    // A valid extension recipe uses a mandatory repeat and optional zero pieces.
    let mut b = tfm(&[], &[(b'x', 3, 0)]);
    let n = b.len();
    let params = n - 28;
    b.splice(params..params, [0, 0, 0, b'a']);
    let words = (b.len() / 4) as u16;
    b[..2].copy_from_slice(&words.to_be_bytes());
    b[20..22].copy_from_slice(&1u16.to_be_bytes());
    Catalog::default().register(b"extensible", &b).unwrap();
    b[params + 3] = 0;
    assert!(Catalog::default().register(b"bad", &b).is_err());
    // TeX checks dimension[0] after scaling; a tiny nonzero fix-word can round0.
    let mut b = basic();
    let start = 4 * (6 + 2 + 128);
    b[start + 3] = 1;
    let mut c = Catalog::default();
    c.register(b"tiny", &b).unwrap();
    assert!(c.scaled(0, Scaled(655360)).is_ok());
    assert!(c.scaled(0, Scaled(2_000_000)).is_err());
}

#[test]
fn dense_metrics_preserve_presence_fields_and_character_ranges() {
    let mut full = tfm(&[], &[]);
    full.splice(32 + 128 * 4..32 + 128 * 4, [1, 0x11, 4, 0].repeat(128));
    let dimensions = 32 + 256 * 4;
    // Index 2 remains present although its scaled width is zero. Character 0
    // is absent, but its height/depth/italic fields must still be observable.
    full[dimensions + 8..dimensions + 12].fill(0);
    for (first, last) in [(0, 255), (64, 127), (192, 255), (1, 0)] {
        let mut bytes = full[..32].to_vec();
        bytes.extend_from_slice(&full[32 + first * 4..32 + (last + 1) * 4]);
        bytes.extend_from_slice(&full[dimensions..]);
        let words = (bytes.len() / 4) as u16;
        bytes[..2].copy_from_slice(&words.to_be_bytes());
        bytes[4..6].copy_from_slice(&(first as u16).to_be_bytes());
        bytes[6..8].copy_from_slice(&(last as u16).to_be_bytes());
        let mut catalog = Catalog::default();
        catalog.register(b"range", &bytes).unwrap();
        for size in [Scaled(655360), Scaled(13_107_201)] {
            let scaled = catalog.scaled(0, size).unwrap();
            let font = &catalog.scaled[scaled];
            for character in 0..=255u8 {
                let c = character as usize;
                let info = &full[32 + c * 4..32 + (c + 1) * 4];
                let fields = (first..=last).contains(&c).then(|| {
                    let value = |index| {
                        let start = dimensions + index * 4;
                        scale(full[start..start + 4].try_into().unwrap(), size).unwrap()
                    };
                    Metrics {
                        width: value(info[0] as usize),
                        height: value(4 + (info[1] >> 4) as usize),
                        depth: value(6 + (info[1] & 15) as usize),
                        italic: value(8 + (info[2] >> 2) as usize),
                    }
                });
                let emitted = fields.filter(|_| info[0] != 0);
                assert_eq!(catalog.metric_fields(scaled, character), fields);
                assert_eq!(catalog.metrics(scaled, character), emitted);
                assert_eq!(font.descriptor().exists(character), emitted.is_some());
                assert_eq!(
                    font.metrics(character),
                    emitted.map(|m| GlyphMetrics {
                        width: m.width.0,
                        height: m.height.0,
                        depth: m.depth.0,
                        italic: m.italic.0,
                    })
                );
            }
        }
    }
}

#[test]
fn byte_character_fields_with_wide_metric_and_program_tables() {
    let [nw, nh, nd, ni, nl, ne] = [300, 257, 258, 260, 256, 256];
    let lf = 6 + 2 + 256 + nw + nh + nd + ni + nl + 1 + ne;
    let mut bytes = Vec::new();
    for n in [lf, 2, 0, 255, nw, nh, nd, ni, nl, 1, ne, 0] {
        bytes.extend_from_slice(&(n as u16).to_be_bytes());
    }
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&(10i32 << 20).to_be_bytes());
    for character in 0..256 {
        let (tag, remainder) = match character {
            2 => (1, 255),
            3 => (2, 255),
            4 => (3, 255),
            _ => (0, 255),
        };
        bytes.extend_from_slice(&[u8::from(character != 0) * 255, 255, 252 | tag, remainder]);
    }
    for (count, index, value) in [
        (nw, 255, 524288i32),
        (nh, 15, 734003),
        (nd, 15, 209715),
        (ni, 63, 104858),
    ] {
        for i in 0..count {
            bytes.extend_from_slice(&(if i == index { value } else { 0 }).to_be_bytes());
        }
    }
    bytes.extend_from_slice(&[128, 1, 128, 0].repeat(nl));
    bytes.extend_from_slice(&0i32.to_be_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 1].repeat(ne));
    let mut catalog = Catalog::default();
    catalog.register(b"wide", &bytes).unwrap();
    assert_eq!(catalog.resources[0].entries[2], 255);
    for size in [Scaled(655360), Scaled(13_107_201)] {
        let scaled = catalog.scaled(0, size).unwrap();
        let expected = Metrics {
            width: scale(524288i32.to_be_bytes(), size).unwrap(),
            height: scale(734003i32.to_be_bytes(), size).unwrap(),
            depth: scale(209715i32.to_be_bytes(), size).unwrap(),
            italic: scale(104858i32.to_be_bytes(), size).unwrap(),
        };
        assert_eq!(catalog.metrics(scaled, 255), Some(expected));
        assert_eq!(catalog.metrics(scaled, 0), None);
        assert_eq!(
            catalog.metric_fields(scaled, 0),
            Some(Metrics {
                width: Scaled::ZERO,
                ..expected
            })
        );
    }
    // Out-of-range fields remain errors before any indexed access. In
    // particular tag/remainder values must not be confused with table lengths.
    for (offset, value) in [(0, 255), (1, 0xf1), (1, 0x1f), (2, 252), (2, 3), (2, 2)] {
        let mut bad = tfm(&[], &[]);
        bad[32 + 4 + offset] = value;
        if offset == 2 && value == 2 {
            bad[32 + 4 + 3] = 128;
        }
        assert_eq!(Catalog::default().register(b"bad", &bad), Err(Error::Font));
    }
}

#[test]
fn raw_table_views_retain_exact_bytes_across_sizes_and_catalog_teardown() {
    let mut bytes = basic();
    bytes.extend_from_slice(&[255; 7]);
    let original = bytes.clone();
    let mut catalog = Catalog::default();
    catalog.register(b"raw", &bytes).unwrap();
    bytes.fill(0);
    let first = catalog.scaled(0, Scaled(655360)).unwrap();
    let second = catalog.scaled(0, Scaled(1310720)).unwrap();
    let fonts = [
        catalog.scaled[first].clone(),
        catalog.scaled[second].clone(),
    ];
    drop(catalog);
    for (font, width) in fonts.iter().zip([491520, 983040]) {
        assert_eq!(font.tfm_bytes(), original);
        assert_eq!(font.metrics(b'a').unwrap().width, width);
        let source = font.source();
        assert_eq!(source.chars().len(), 128);
        assert_eq!(source.program().len(), 5);
        assert_eq!(source.program()[0], [0, b'i', 0, 12]);
        assert_eq!(source.program()[4], [128, b'V', 128, 0]);
        assert_eq!(
            source.chars().as_flattened().as_ptr(),
            font.tfm_bytes()[source.chars_start * 4..].as_ptr()
        );
        assert_eq!(
            source.program().as_flattened().as_ptr(),
            font.tfm_bytes()[source.program_start * 4..].as_ptr()
        );
    }
    // Extra bytes cannot supply a program word beyond the declared table.
    let mut bad = tfm(&[[128, b'a', 128, 0]], &[(b'f', 1, 1)]);
    bad.extend_from_slice(&[128, b'a', 128, 0]);
    assert_eq!(Catalog::default().register(b"bad", &bad), Err(Error::Font));
}
