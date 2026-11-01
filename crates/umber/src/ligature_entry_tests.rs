use crate::{font_test_data::tfm, tfm::Catalog};

#[test]
fn prepared_entries_preserve_restart_stop_and_boundary_semantics() {
    let bytes = tfm(
        &[
            [255, 200, 0, 1],
            [129, b'i', 0, 2],
            [0, b'i', 0, 12],
            [128, b'l', 128, 0],
            [255, 0, 0, 2],
        ],
        &[(b'f', 1, 0), (b'g', 1, 1), (0, 1, 2), (b'h', 1, 3)],
    );
    let mut catalog = Catalog::default();
    catalog.register(b"entry", &bytes).unwrap();
    let r = &catalog.resources[0];
    // One restart reaches another restart word; it must stop, not follow it.
    assert_eq!(r.lig_instruction(b'f' as u16, b'i' as u16, None), None);
    let ligature = Some([0, b'i', 0, 12]);
    assert_eq!(r.lig_instruction(b'g' as u16, b'i' as u16, None), ligature);
    // Width index zero does not erase the character's program metadata.
    assert_eq!(r.chars[0].width, 0);
    assert_eq!(r.lig_instruction(0, b'i' as u16, None), ligature);
    assert_eq!(
        r.lig_instruction(b'h' as u16, b'l' as u16, None),
        Some([128, b'l', 128, 0])
    );
    assert_eq!(r.lig_instruction(256, b'i' as u16, None), None);
    assert_eq!(r.lig_instruction(128, b'i' as u16, None), None);
    assert_eq!(r.lig_instruction(b'a' as u16, b'i' as u16, None), None);
    // Explicit boundary starts bypass character entry preparation entirely.
    assert_eq!(r.boundary_label, Some(2));
    assert_eq!(
        r.lig_instruction(256, b'i' as u16, r.boundary_label),
        ligature
    );
    assert_eq!(r.lig_instruction(256, b'i' as u16, Some(1)), None);
    assert_eq!(r.lig_instruction(256, 256, Some(2)), None);
}

#[test]
fn prepared_entries_cover_large_targets_and_empty_character_ranges() {
    let mut program = vec![[128, 1, 128, 0]; 32000];
    program[0] = [129, 0, 124, 255];
    let mut catalog = Catalog::default();
    catalog
        .register(b"long", &tfm(&program, &[(b'f', 1, 0)]))
        .unwrap();
    let r = &catalog.resources[0];
    assert_eq!(r.entries[b'f' as usize], 31999);
    assert_eq!(
        r.lig_instruction(b'f' as u16, 1, None),
        Some(program[31999])
    );

    // Both TFM encodings of an empty character range leave no entry to read.
    for (first, last) in [(1u16, 0u16), (256, 255)] {
        let mut bytes = tfm(&[], &[]);
        bytes.drain(32..32 + 128 * 4);
        let words = (bytes.len() / 4) as u16;
        bytes[..2].copy_from_slice(&words.to_be_bytes());
        bytes[4..6].copy_from_slice(&first.to_be_bytes());
        bytes[6..8].copy_from_slice(&last.to_be_bytes());
        let mut catalog = Catalog::default();
        catalog.register(b"empty", &bytes).unwrap();
        let r = &catalog.resources[0];
        assert!(r.entries.iter().all(|&i| i == u16::MAX));
        for left in 0..=256 {
            assert_eq!(r.lig_instruction(left, 1, None), None);
        }
    }
}
