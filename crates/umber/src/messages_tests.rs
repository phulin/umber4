use super::*;

#[test]
fn prefixes_share_bytes_and_abandoned_blocks_release_and_reuse() {
    let mut live = Messages::default();
    for i in 0..35 {
        live.push(vec![i]);
    }
    let prefix = live.clone();
    let bytes = prefix.blocks[0].data.as_ref().unwrap()[0].as_ptr();
    let pool = Rc::downgrade(&live.pool);
    let mut reused = None;
    for _ in 0..20 {
        live.push(b"branch".to_vec());
        assert_eq!(live.blocks[0].data.as_ref().unwrap()[0].as_ptr(), bytes);
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
    published[0][0] = 200;
    assert_eq!(prefix.copy()[0], [0]);
    drop(prefix);
    assert_eq!(
        live.into_vec(),
        (0..35).map(|i| vec![i]).collect::<Vec<_>>()
    );
    assert!(pool.upgrade().is_none());
}
