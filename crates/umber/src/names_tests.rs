use super::SpellingStore;

#[test]
fn published_spellings_survive_suffix_writes_and_chunk_growth() {
    let mut store = SpellingStore::default();
    let empty = store.append(b"");
    // SAFETY: the empty descriptor uses a static empty slice.
    assert!(unsafe { empty.as_ref() }.is_empty());
    let first = store.append(b"retained");
    // SAFETY: first is initialized and immutable. store owns its chunk until
    // after this view's last read; appends initialize only disjoint cells.
    let retained = unsafe { first.as_ref() };
    store.append(b"neighbor");
    assert_eq!(retained, b"retained");
    let mut spellings = Vec::new();
    for (index, length) in [4080, 4096, 4097, 8193].into_iter().enumerate() {
        let expected = vec![index as u8; length];
        let spelling = store.append(&expected);
        spellings.push((spelling, expected));
    }
    for index in 0..12 {
        store.append(&vec![index; 4096]);
    }
    assert_eq!(retained, b"retained");
    for (spelling, expected) in spellings {
        // SAFETY: directory growth preserves every initialized boxed chunk,
        // and the store remains alive through each descriptor's last read.
        assert_eq!(unsafe { spelling.as_ref() }, expected);
    }
}
