use super::*;

#[test]
fn prefixes_share_bytes_and_abandoned_blocks_release_and_reuse() {
    let mut live = Diagnostics::default();
    for i in 0..35 {
        live.push(Diagnostic::Message(vec![i]));
    }
    let prefix = live.clone();
    let bytes = prefix.blocks[0].data.as_ref().unwrap()[0].bytes().as_ptr();
    let pool = Rc::downgrade(&live.pool);
    let mut reused = None;
    for _ in 0..20 {
        live.push(Diagnostic::Message(b"branch".to_vec()));
        assert_eq!(
            live.blocks[0].data.as_ref().unwrap()[0].bytes().as_ptr(),
            bytes
        );
        let pointer = Rc::as_ptr(live.blocks.last().unwrap().data.as_ref().unwrap());
        if let Some(reused) = reused {
            assert_eq!(pointer, reused);
        }
        reused = Some(pointer);
        assert_eq!(live.copy().len(), 36);
        assert_eq!(prefix.copy().len(), 35);
        live.clone_from(&prefix);
    }
    // Published host bytes can be changed or retained independently.
    let mut published = live.copy();
    if let Diagnostic::Message(bytes) = &mut published[0] {
        bytes[0] = 200;
    }
    assert_eq!(prefix.copy()[0].bytes(), [0]);
    drop(prefix);
    assert_eq!(
        live.into_vec(),
        (0..35)
            .map(|i| Diagnostic::Message(vec![i]))
            .collect::<Vec<_>>()
    );
    assert!(pool.upgrade().is_none());
}

#[test]
fn typed_write_prefixes_share_payloads_and_release_stores_before_public_bytes() {
    let mut live = Diagnostics::default();
    live.push(Diagnostic::Message(b"MESSAGE".to_vec()));
    live.push(Diagnostic::Write {
        destination: DiagnosticDestination::LogOnly,
        bytes: b"LOG".to_vec(),
    });
    let saved = live.clone();
    let pointer = saved.blocks[0].data.as_ref().unwrap()[1].bytes().as_ptr();
    let pool = Rc::downgrade(&live.pool);
    live.push(Diagnostic::Write {
        destination: DiagnosticDestination::TerminalAndLog,
        bytes: b"TERM".to_vec(),
    });
    assert_eq!(
        saved.blocks[0].data.as_ref().unwrap()[1].bytes().as_ptr(),
        pointer
    );
    let published = live.into_vec();
    assert_eq!(saved.copy().len(), 2);
    drop(saved);
    assert!(pool.upgrade().is_none());
    assert_eq!(published[0].bytes(), b"MESSAGE");
    assert_eq!(published[1].destination(), DiagnosticDestination::LogOnly);
    assert_eq!(
        published[2].destination(),
        DiagnosticDestination::TerminalAndLog
    );
    assert_eq!(published[2].bytes(), b"TERM");
}
