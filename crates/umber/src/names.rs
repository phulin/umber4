use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

type ByteCell = UnsafeCell<MaybeUninit<u8>>;

// Each spelling is initialized once, then immutable. Names owns every chunk
// until all spelling descriptors are dropped; moving its directory leaves the
// boxed byte allocations in place. Unused suffix cells remain writable.
#[derive(Default)]
pub(crate) struct SpellingStore {
    chunks: Vec<Box<[ByteCell]>>,
    next: *mut u8,
    available: usize,
}
impl SpellingStore {
    pub(crate) fn append(&mut self, name: &[u8]) -> NonNull<[u8]> {
        if name.is_empty() {
            return NonNull::from(b"".as_slice());
        }
        if name.len() > self.available {
            self.grow(name.len());
        }
        let start = self.next;
        // SAFETY: grow reserves enough unused cells in one allocation. The
        // input is a separate reader buffer; published spellings occupy only
        // earlier cells. Copy initializes this entire range before publication.
        unsafe {
            std::ptr::copy_nonoverlapping(name.as_ptr(), start, name.len());
            self.next = start.add(name.len());
        }
        self.available -= name.len();
        // SAFETY: a nonempty reservation has a nonnull allocation pointer.
        unsafe { NonNull::new_unchecked(std::ptr::slice_from_raw_parts_mut(start, name.len())) }
    }
    #[inline(never)]
    fn grow(&mut self, minimum: usize) {
        let length = minimum.max(4096);
        let chunk = Box::<[ByteCell]>::new_uninit_slice(length);
        // SAFETY: ByteCell permits uninitialized bytes, has byte layout, and
        // has no destructor. append initializes each range before publishing it.
        self.chunks.push(unsafe { chunk.assume_init() });
        let chunk = self.chunks.last().unwrap();
        self.next = UnsafeCell::raw_get(chunk.as_ptr()).cast::<u8>();
        self.available = length;
    }
}

#[cfg(test)]
#[path = "names_tests.rs"]
mod tests;
