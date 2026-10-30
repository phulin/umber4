use super::*;

#[test]
fn typed_blocks_amortize_live_growth_and_reuse_capacity() {
    let store = Rc::new(Store::default());
    let mut buffers: Vec<_> = (0..193)
        .map(|index| {
            let mut buffer = Buffer::new(&store);
            buffer.push(index);
            buffer
        })
        .collect();
    // Every backing page supplies many simultaneously live objects.
    assert!(store.pages() < buffers.len() / 8);
    let held = buffers.swap_remove(64);
    let held_pointer = held.as_ptr();
    let pages = store.pages();
    drop(buffers);
    for index in 0..193 {
        let mut buffer = Buffer::new(&store);
        buffer.push(index);
        assert_eq!(&*held, &[64]);
    }
    assert_eq!(store.pages(), pages);
    drop(store);
    assert_eq!(held.as_ptr(), held_pointer);
    assert_eq!(&*held, &[64]);
}

#[test]
fn moving_and_clearing_blocks_release_each_initialized_owner_once() {
    let store = Rc::new(Store::default());
    let owner = Rc::new(());
    let mut buffer = Buffer::new(&store);
    for _ in 0..257 {
        buffer.push(Rc::clone(&owner));
    }
    assert_eq!(Rc::strong_count(&owner), 258);
    buffer.resize(17, Rc::clone(&owner));
    assert_eq!(Rc::strong_count(&owner), 18);
    buffer.clear();
    assert_eq!(Rc::strong_count(&owner), 1);
    let pages = store.pages();
    for _ in 0..257 {
        buffer.push(Rc::clone(&owner));
    }
    assert_eq!(store.pages(), pages);
    drop(store);
    drop(buffer);
    assert_eq!(Rc::strong_count(&owner), 1);
}

#[test]
fn failed_partial_initialization_keeps_owned_prefix_and_recycles_after_drop() {
    struct FallibleClone {
        owner: Rc<()>,
        remaining: Rc<std::cell::Cell<usize>>,
    }
    impl Clone for FallibleClone {
        fn clone(&self) -> Self {
            let remaining = self.remaining.get();
            assert_ne!(remaining, 0, "test clone failure");
            self.remaining.set(remaining - 1);
            Self {
                owner: Rc::clone(&self.owner),
                remaining: Rc::clone(&self.remaining),
            }
        }
    }
    let store = Rc::new(Store::default());
    let owner = Rc::new(());
    let mut buffer = Buffer::new(&store);
    buffer.push(FallibleClone {
        owner: Rc::clone(&owner),
        remaining: Rc::new(std::cell::Cell::new(0)),
    });
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        buffer.resize(
            65,
            FallibleClone {
                owner: Rc::clone(&owner),
                remaining: Rc::new(std::cell::Cell::new(2)),
            },
        );
    }));
    assert!(result.is_err());
    assert_eq!(buffer.len(), 3);
    assert_eq!(Rc::strong_count(&owner), 4);
    let pointer = buffer.as_ptr();
    let pages = store.pages();
    drop(buffer);
    assert_eq!(Rc::strong_count(&owner), 1);
    let mut replacement = Buffer::new(&store);
    replacement.resize(
        65,
        FallibleClone {
            owner: Rc::clone(&owner),
            remaining: Rc::new(std::cell::Cell::new(65)),
        },
    );
    assert_eq!(replacement.as_ptr(), pointer);
    assert_eq!(store.pages(), pages);
    drop(store);
    drop(replacement);
    assert_eq!(Rc::strong_count(&owner), 1);
}

#[test]
fn removed_and_partially_constructed_elements_keep_exact_ownership() {
    let store = Rc::new(Store::default());
    let owner = Rc::new(());
    let mut buffer = Buffer::new(&store);
    buffer.resize_with(9, || Rc::clone(&owner));
    let removed = buffer.remove(3);
    assert_eq!(buffer.len(), 8);
    assert_eq!(Rc::strong_count(&owner), 10);
    drop(removed);
    drop(buffer.pop());
    assert_eq!(Rc::strong_count(&owner), 8);
    let mut remaining = 2;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        buffer.resize_with(65, || {
            assert_ne!(remaining, 0, "test construction failure");
            remaining -= 1;
            Rc::clone(&owner)
        });
    }));
    assert!(result.is_err());
    assert_eq!(buffer.len(), 9);
    assert_eq!(Rc::strong_count(&owner), 10);
    buffer.resize_with(1, || unreachable!());
    assert_eq!(Rc::strong_count(&owner), 2);
    drop(store);
    drop(buffer);
    assert_eq!(Rc::strong_count(&owner), 1);
}
