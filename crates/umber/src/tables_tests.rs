use super::*;

#[test]
fn retained_pages_isolate_mutations_and_caches_release_payloads() {
    let payload = Rc::new(123);
    let weak = Rc::downgrade(&payload);
    let mut live = Table::<Option<Rc<i32>>>::new(128);
    let pool = Rc::downgrade(&live.pool);
    live[63] = Some(payload);
    let old = live.clone();
    live[63] = None;
    live[64] = Some(Rc::new(456));
    assert_eq!(old[63].as_deref(), Some(&123));
    assert!(old[64].is_none());
    drop(old);
    assert!(
        weak.upgrade().is_none(),
        "cached pages must release payloads"
    );
    drop(live);
    assert!(pool.upgrade().is_none(), "page caches must not form cycles");
}

#[test]
fn repeated_branch_writes_reuse_page_records_and_default_new_names() {
    let mut live = Table::<Option<Rc<i32>>>::new(1);
    let old = live.clone();
    let mut reused = None;
    for value in 0..20 {
        live[0] = Some(Rc::new(value));
        let pointer = Rc::as_ptr(live.pages[0].data.as_ref().unwrap());
        if let Some(reused) = reused {
            assert_eq!(pointer, reused);
        }
        reused = Some(pointer);
        assert!(old[0].is_none());
        assert!(old[1000].is_none());
        live.clone_from(&old);
    }
    live[1000] = Some(Rc::new(17));
    assert_eq!(live[1000].as_deref(), Some(&17));
    assert!(old[1000].is_none());
}
