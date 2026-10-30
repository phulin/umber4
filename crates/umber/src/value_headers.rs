use super::*;
use std::ptr::NonNull;

const SLOTS: usize = 64;

#[repr(C)]
pub(crate) struct Value {
    pub tokens: Tokens,
    pub(super) definition: Definition,
    pub id: u32,
}
impl Value {
    #[inline]
    pub fn definition(&self) -> &Definition {
        &self.definition
    }
}

#[repr(C)]
pub(super) struct Slot {
    owners: Cell<usize>,
    pub(super) published: Cell<bool>,
    pub(super) value: UnsafeCell<Value>,
    slab: NonNull<Slab>,
}
struct Slab {
    references: Cell<usize>,
    registered: Cell<bool>,
    pool: Weak<Pool>,
    slots: [Slot; SLOTS],
}
struct SlabOwner(NonNull<Slab>);
impl SlabOwner {
    fn new(pool: &Rc<Pool>, first: u32) -> Self {
        let allocation = Box::<Slab>::new_uninit();
        let pointer = NonNull::new(Box::into_raw(allocation).cast::<Slab>()).unwrap();
        // SAFETY: initialize the complete allocation in place before exposing
        // any slot. The self pointer addresses this stable allocation and is
        // only dereferenced while a registry or occupied-slot reference exists.
        unsafe {
            let raw = pointer.as_ptr();
            std::ptr::addr_of_mut!((*raw).references).write(Cell::new(0));
            std::ptr::addr_of_mut!((*raw).registered).write(Cell::new(true));
            std::ptr::addr_of_mut!((*raw).pool).write(Rc::downgrade(pool));
            let slots = std::ptr::addr_of_mut!((*raw).slots).cast::<Slot>();
            for index in 0..SLOTS {
                slots.add(index).write(Slot {
                    value: UnsafeCell::new(Value {
                        id: first + index as u32,
                        tokens: Tokens {
                            rest: Buffer::new(&pool.pieces),
                            ..Tokens::default()
                        },
                        definition: Definition {
                            parameters: Buffer::new(&pool.parameters),
                            failure: Buffer::new(&pool.failures),
                            ..Definition::default()
                        },
                    }),
                    published: Cell::new(false),
                    owners: Cell::new(0),
                    slab: pointer,
                });
            }
        }
        Self(pointer)
    }
    fn slab(&self) -> &Slab {
        // SAFETY: this handle retains the initialized allocation.
        unsafe { self.0.as_ref() }
    }
}
impl SlabOwner {
    #[cold]
    #[inline(never)]
    fn unregister(&self) {
        let slab = self.slab();
        // The registry is no longer borrowed. Each occupied slot now takes one
        // retention reference, irrespective of its aliases. Counting a fixed
        // 64-slot array cannot overflow; only alias counters grow unboundedly.
        let references = slab
            .slots
            .iter()
            .filter(|slot| slot.owners.get() != 0)
            .count();
        slab.references.set(references);
        slab.registered.set(false);
        if references == 0 {
            self.deallocate();
        }
    }
    #[cold]
    #[inline(never)]
    fn deallocate(&self) {
        // SAFETY: no registry or occupied slot remains. No later operation in
        // the releasing destructor accesses slab. Every slot is initialized and
        // contains no live external owner; metadata capacity drops here.
        unsafe { drop(Box::from_raw(self.0.as_ptr())) };
    }
}
impl Drop for SlabOwner {
    #[inline]
    fn drop(&mut self) {
        let slab = self.slab();
        if slab.registered.get() {
            self.unregister();
            return;
        }
        let references = slab.references.get();
        slab.references.set(references - 1);
        if references == 1 {
            self.deallocate();
        }
    }
}

// One pointer per semantic owner. Aliases change the embedded slot counter;
// slab references are needed only after the registry has disappeared.
pub(crate) struct Owner(NonNull<Slot>);
impl Owner {
    #[inline]
    pub(super) fn slot(&self) -> &Slot {
        // SAFETY: at least this owner keeps the slot occupied and slab alive.
        unsafe { self.0.as_ref() }
    }
}
impl Clone for Owner {
    #[inline]
    fn clone(&self) -> Self {
        let slot = self.slot();
        // As in Rc, abort immediately if the embedded count wraps. Publishing
        // the increment first permits one in-memory increment on native paths.
        let owners = slot.owners.get().wrapping_add(1);
        slot.owners.set(owners);
        if owners == 0 {
            std::process::abort();
        }
        Self(self.0)
    }
}
impl std::ops::Deref for Owner {
    type Target = Value;
    #[inline]
    fn deref(&self) -> &Value {
        // SAFETY: views borrow a live owner. Mutation is limited to exclusive
        // unpublished builders and final-owner release, after all views end.
        unsafe { &*self.slot().value.get() }
    }
}
impl Drop for Owner {
    #[inline]
    fn drop(&mut self) {
        let slot = self.slot();
        let owners = slot.owners.get();
        slot.owners.set(owners - 1);
        if owners == 1 {
            drop(slot.release());
        }
    }
}
impl Slot {
    #[inline(never)]
    fn release(&self) -> Option<SlabOwner> {
        self.published.set(false);
        // SAFETY: final external owner has ended all borrowed views. The
        // registry or transferred slot reference retains the slab but has no
        // reader of this slot's contents.
        let value = unsafe { &mut *self.value.get() };
        // SAFETY: this occupied slot retains the slab through this release.
        let slab = unsafe { self.slab.as_ref() };
        value.definition.clear();
        let pool = slab.pool.upgrade();
        if let Some(pool) = &pool
            && let Some(last) = value.tokens.last()
        {
            pool.rewind(last, last.capacity);
        }
        value.tokens.first = None;
        value.tokens.rest.clear();
        value.tokens.len = 0;
        if let Some(pool) = pool {
            pool.ids.borrow_mut().push(value.id);
        }
        // The registry keeps every slab alive without per-checkout bookkeeping.
        // After registry teardown, return before deallocation: the release call
        // borrows this slot. Its caller drops the transferred reference only
        // after that borrow has ended.
        if slab.registered.get() {
            None
        } else {
            Some(SlabOwner(self.slab))
        }
    }
}

#[derive(Default)]
pub(crate) struct Values {
    pub(super) pool: Rc<Pool>,
    slabs: Vec<SlabOwner>,
    slots: Vec<NonNull<Slot>>,
}
impl Values {
    pub(super) fn slot(&self, id: u32) -> &Slot {
        // SAFETY: the registry owns every slab named by this dense directory.
        unsafe { self.slots[id as usize].as_ref() }
    }
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.slots.len()
    }
    #[cfg(test)]
    pub(super) fn slots(&self) -> impl Iterator<Item = &Slot> {
        self.slots.iter().map(|slot| {
            // SAFETY: the registry retains every slot's slab.
            unsafe { slot.as_ref() }
        })
    }
    pub fn packet(&self) -> Packet {
        Packet {
            arguments: Buffer::new(&self.pool.arguments),
            owners: Buffer::new(&self.pool.owners),
        }
    }
    pub fn prepare_packet(&self, packet: &mut Packet, count: usize) {
        packet.arguments.resize_with(count, || Sequence {
            rest: Buffer::new(&self.pool.spans),
            ..Sequence::default()
        });
    }
    pub fn name_buffer(&self) -> Buffer<u8> {
        Buffer::new(&self.pool.name_bytes)
    }
    pub fn builder(&mut self, class: Class) -> Result<Builder> {
        let id = self
            .pool
            .ids
            .borrow_mut()
            .pop()
            .unwrap_or(self.slots.len() as u32);
        if id >= SINGLE {
            return Err(Error::Capacity);
        }
        if id as usize == self.slots.len() {
            if self.slots.len().is_multiple_of(SLOTS) {
                self.slabs.push(SlabOwner::new(&self.pool, id));
            }
            let slab = self.slabs.last().unwrap().slab();
            let slot = &slab.slots[self.slots.len() % SLOTS];
            self.slots.push(NonNull::from(slot));
        }
        let slot = self.slot(id);
        debug_assert_eq!(slot.owners.get(), 0);
        // The registry retains this slab. Its cold teardown transfers retention
        // to occupied slots, including any unpublished builders that survive it.
        slot.owners.set(1);
        let owner = Owner(NonNull::from(slot));
        Ok(Builder {
            owner,
            pool: Rc::clone(&self.pool),
            class,
            base: std::ptr::null_mut(),
            next: std::ptr::null_mut(),
            limit: std::ptr::null_mut(),
            start: 0,
        })
    }
    #[inline]
    pub fn get(&self, id: u32) -> Owner {
        let slot = self.slot(id);
        assert!(slot.published.get(), "retained published token value");
        let owner = std::mem::ManuallyDrop::new(Owner(NonNull::from(slot)));
        Owner::clone(&owner)
    }
    fn tokens(&self, id: u32) -> &Tokens {
        let slot = self.slot(id);
        assert!(slot.published.get(), "retained published token value");
        // SAFETY: these non-reentrant queries cannot release an owner. Raw
        // windows escaping this call require a separate caller-held owner.
        unsafe { &(*slot.value.get()).tokens }
    }
    pub fn token(&self, id: u32, position: u32) -> Token {
        self.tokens(id).get(position)
    }
    pub fn window(&self, span: Span, start: u32) -> (*const Token, *const Token, u32) {
        self.tokens(span.store).window(start, span.end)
    }
    #[cfg(test)]
    pub fn retained(&self) -> (usize, usize, usize) {
        let live = self.slots().filter(|slot| slot.published.get()).count();
        (live, self.slots.len(), self.pool.free.borrow().len())
    }
}
