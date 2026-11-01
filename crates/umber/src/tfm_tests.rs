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
