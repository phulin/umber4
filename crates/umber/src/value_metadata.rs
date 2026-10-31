//! Typed size-class backing for value, invocation, and scanner metadata.
use std::cell::{RefCell, UnsafeCell};
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::rc::Rc;

struct Cells<T>(NonNull<[UnsafeCell<MaybeUninit<T>>]>);
impl<T> Drop for Cells<T> {
    fn drop(&mut self) {
        // SAFETY: created once from Box::into_raw; every initialized element
        // was released by its buffer before the store's last owner disappears.
        unsafe { drop(Box::from_raw(self.0.as_ptr())) };
    }
}
struct Bucket<T> {
    pages: Vec<Cells<T>>,
    free: Vec<NonNull<T>>,
}
impl<T> Default for Bucket<T> {
    fn default() -> Self {
        Self {
            pages: Vec::new(),
            free: Vec::new(),
        }
    }
}
pub(crate) struct Store<T> {
    buckets: RefCell<Vec<Bucket<T>>>,
}
impl<T> Default for Store<T> {
    fn default() -> Self {
        Self {
            buckets: RefCell::default(),
        }
    }
}
impl<T> Store<T> {
    fn class(capacity: u32) -> usize {
        // Includes a final u32::MAX class if rounding to 2^32 cannot fit.
        (32 - (capacity - 1).leading_zeros() - 2) as usize
    }
    fn take(&self, capacity: u32) -> NonNull<T> {
        assert_ne!(size_of::<T>(), 0);
        let mut buckets = self.buckets.borrow_mut();
        let class = Self::class(capacity);
        let classes = buckets.len().max(class + 1);
        buckets.resize_with(classes, Bucket::default);
        let bucket = &mut buckets[class];
        if let Some(pointer) = bucket.free.pop() {
            return pointer;
        }
        let bytes = (capacity as usize)
            .checked_mul(size_of::<T>())
            .filter(|bytes| *bytes <= isize::MAX as usize)
            .expect("metadata capacity");
        let blocks = (4096 / bytes).clamp(1, 64);
        let count = (capacity as usize)
            .checked_mul(blocks)
            .expect("metadata capacity");
        // Reserve descriptors before allocating backing. No user code or
        // fallible operation runs after initialized metadata is transferred.
        let all_blocks = (bucket.pages.len() + 1)
            .checked_mul(blocks)
            .expect("metadata capacity");
        bucket.free.reserve(all_blocks - bucket.free.len());
        bucket.pages.reserve(1);
        let allocation = Box::<[UnsafeCell<MaybeUninit<T>>]>::new_uninit_slice(count);
        // SAFETY: every element is UnsafeCell<MaybeUninit<T>>, which permits
        // uninitialized contents. Buffers initialize only their own prefixes.
        let cells = unsafe { allocation.assume_init() };
        // Raw ownership prevents moving Box handles in the page directory from
        // retagging their allocations and invalidating published block views.
        let cells = Cells(NonNull::new(Box::into_raw(cells)).unwrap());
        let base = cells.0.as_ptr().cast::<UnsafeCell<MaybeUninit<T>>>();
        for block in 0..blocks {
            // SAFETY: each block starts inside this stable page and covers
            // capacity disjoint T cells with T's size and alignment.
            let pointer =
                unsafe { UnsafeCell::raw_get(base.add(block * capacity as usize)).cast::<T>() };
            bucket.free.push(NonNull::new(pointer).unwrap());
        }
        bucket.pages.push(cells);
        bucket.free.pop().unwrap()
    }
    fn recycle(&self, pointer: NonNull<T>, capacity: u32) {
        // The free directory reserved room for every block when backing grew.
        self.buckets.borrow_mut()[Self::class(capacity)]
            .free
            .push(pointer);
    }
    #[cfg(test)]
    pub(crate) fn pages(&self) -> usize {
        self.buckets.borrow().iter().map(|b| b.pages.len()).sum()
    }
}

// Keep the native Vec footprint without allocating a shared record per buffer.
// The store owns backing; this buffer exclusively owns its initialized prefix.
pub(crate) struct Buffer<T> {
    pointer: NonNull<T>,
    length: u32,
    capacity: u32,
    store: Option<Rc<Store<T>>>,
}
impl<T> Default for Buffer<T> {
    fn default() -> Self {
        Self {
            pointer: NonNull::dangling(),
            length: 0,
            capacity: 0,
            store: None,
        }
    }
}
impl<T> Buffer<T> {
    pub fn new(store: &Rc<Store<T>>) -> Self {
        Self {
            store: Some(Rc::clone(store)),
            ..Self::default()
        }
    }
    #[cfg(test)]
    pub fn capacity(&self) -> usize {
        self.capacity as usize
    }
    #[inline(never)]
    fn grow(&mut self, requested: u32) {
        let capacity = requested
            .max(4)
            .checked_next_power_of_two()
            .unwrap_or(u32::MAX);
        let store = self.store.as_ref().expect("metadata store");
        let pointer = store.take(capacity);
        // SAFETY: this buffer exclusively owns length initialized cells. The
        // new disjoint block has sufficient capacity. Copy transfers ownership;
        // the old prefix is no longer initialized after its block is returned.
        unsafe {
            std::ptr::copy_nonoverlapping(
                self.pointer.as_ptr(),
                pointer.as_ptr(),
                self.length as usize,
            );
        }
        if self.capacity != 0 {
            store.recycle(self.pointer, self.capacity);
        }
        self.pointer = pointer;
        self.capacity = capacity;
    }
    pub fn reserve(&mut self, additional: usize) {
        let extra = u32::try_from(additional).expect("metadata capacity");
        let length = self.length.checked_add(extra).expect("metadata capacity");
        if length > self.capacity {
            self.grow(length);
        }
    }
    #[inline]
    pub fn push(&mut self, value: T) {
        if self.length == self.capacity {
            let length = self.length.checked_add(1).expect("metadata capacity");
            self.grow(length);
        }
        // SAFETY: the unused cell belongs to this buffer. Publish its length
        // only after initialization; there is no user callback between them.
        // length < capacity also proves the increment fits u32, including when
        // the final size class has capacity u32::MAX.
        unsafe { self.pointer.as_ptr().add(self.length as usize).write(value) };
        self.length += 1;
    }
    pub fn clear(&mut self) {
        let length = std::mem::take(&mut self.length);
        // SAFETY: exactly this initialized prefix belongs to the buffer. Reset
        // before dropping it so unwinding cannot expose or drop it again. For
        // elements without destructors, clear is just the length reset.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                self.pointer.as_ptr(),
                length as usize,
            ));
        }
    }
    fn truncate(&mut self, length: u32) {
        if length >= self.length {
            return;
        }
        let previous = std::mem::replace(&mut self.length, length);
        // SAFETY: the removed suffix is initialized and no longer exposed by
        // the buffer, even if a destructor unwinds.
        unsafe {
            std::ptr::drop_in_place(std::ptr::slice_from_raw_parts_mut(
                self.pointer.as_ptr().add(length as usize),
                (previous - length) as usize,
            ));
        }
    }
    pub fn resize_with(&mut self, length: usize, mut value: impl FnMut() -> T) {
        let length = u32::try_from(length).expect("metadata capacity");
        if length > self.capacity {
            self.grow(length);
        }
        while self.length < length {
            self.push(value());
        }
        self.truncate(length);
    }
    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.length == 0 {
            return None;
        }
        self.length -= 1;
        // SAFETY: transfer the final initialized element out of the prefix.
        Some(unsafe { self.pointer.as_ptr().add(self.length as usize).read() })
    }
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.length as usize);
        // SAFETY: index is initialized and owned exclusively by this buffer.
        // Transfer it to the result, then move the remaining initialized suffix
        // over that cell. The former final cell is no longer initialized.
        unsafe {
            let pointer = self.pointer.as_ptr().add(index);
            let value = pointer.read();
            self.length -= 1;
            std::ptr::copy(pointer.add(1), pointer, self.length as usize - index);
            value
        }
    }
    pub fn extend_from_slice(&mut self, values: &[T])
    where
        T: Copy,
    {
        let extra = u32::try_from(values.len()).expect("metadata capacity");
        let length = self.length.checked_add(extra).expect("metadata capacity");
        if length > self.capacity {
            self.grow(length);
        }
        // SAFETY: the disjoint source is initialized and the destination is
        // this buffer's unused suffix. Publish the new prefix after the copy.
        unsafe {
            std::ptr::copy_nonoverlapping(
                values.as_ptr(),
                self.pointer.as_ptr().add(self.length as usize),
                values.len(),
            );
        }
        self.length = length;
    }
    pub fn resize(&mut self, length: usize, value: T)
    where
        T: Clone,
    {
        let length = u32::try_from(length).expect("metadata capacity");
        if length > self.capacity {
            self.grow(length);
        }
        while self.length < length {
            self.push(value.clone());
        }
        self.truncate(length);
    }
    pub fn extend(&mut self, values: impl IntoIterator<Item = T>) {
        for value in values {
            self.push(value);
        }
    }
}
impl<T: Clone> Clone for Buffer<T> {
    fn clone(&self) -> Self {
        let mut buffer = Self {
            store: self.store.clone(),
            ..Self::default()
        };
        buffer.reserve(self.len());
        buffer.extend(self.iter().cloned());
        buffer
    }
    fn clone_from(&mut self, source: &Self) {
        self.clear();
        if self.store.is_none() {
            self.store = source.store.clone();
        }
        self.reserve(source.len());
        self.extend(source.iter().cloned());
    }
}
impl<T> std::ops::Deref for Buffer<T> {
    type Target = [T];
    #[inline]
    fn deref(&self) -> &[T] {
        // SAFETY: length covers exactly the initialized, exclusively owned
        // prefix; the retained store keeps its stable backing allocation alive.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.length as usize) }
    }
}
impl<T> std::ops::DerefMut for Buffer<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        // SAFETY: a mutable buffer borrow excludes every other access to this
        // initialized prefix. Other buffers own disjoint blocks of its page.
        unsafe { std::slice::from_raw_parts_mut(self.pointer.as_ptr(), self.length as usize) }
    }
}
impl<'a, T> IntoIterator for &'a Buffer<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        self.clear();
        if self.capacity != 0 {
            self.store
                .as_ref()
                .unwrap()
                .recycle(self.pointer, self.capacity);
        }
    }
}

#[cfg(test)]
#[path = "value_metadata_tests.rs"]
mod tests;
