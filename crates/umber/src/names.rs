use crate::token::{CS, NAMED_BASE, NULL_CS, SINGLE_BASE};
use crate::{Error, Result};
use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

const NULL: usize = NULL_CS as usize;
const SINGLE: usize = SINGLE_BASE as usize;
const NAMED: usize = NAMED_BASE as usize;
static BYTE_NAMES: [[u8; 1]; 256] = {
    let mut names = [[0]; 256];
    let mut byte = 0;
    while byte < names.len() {
        names[byte][0] = byte as u8;
        byte += 1;
    }
    names
};
pub(crate) struct Names {
    spellings: Vec<NonNull<[u8]>>,
    // Each bucket packs a 32-bit hash fragment above its stable name ID.
    buckets: Vec<u64>,
    bytes: SpellingStore,
}
impl Names {
    pub const HASH_START: u64 = 0xcbf29ce484222325;
    #[inline(always)]
    pub fn hash_byte(hash: u64, byte: u8) -> u64 {
        (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
    }
    pub fn new() -> Self {
        Self {
            spellings: Vec::with_capacity(512),
            buckets: vec![0; 1024],
            bytes: SpellingStore::default(),
        }
    }
    pub fn len(&self) -> usize {
        NAMED + self.spellings.len()
    }
    pub fn spelling(&self, id: usize) -> &[u8] {
        // Both fixed symbol ranges encode their byte directly in the ID.
        // Multi-byte spellings are initialized immutable ranges in bytes.
        match id {
            0 => b"relax",
            NULL => b"",
            SINGLE..NAMED => &BYTE_NAMES[(id - SINGLE) & 255],
            // SAFETY: every named entry is published after its complete range
            // is initialized. bytes owns that stable chunk for Names' lifetime;
            // this view's borrow ends before Names can be dropped.
            _ => unsafe { self.spellings[id - NAMED].as_ref() },
        }
    }
    pub fn intern(&mut self, name: &[u8]) -> Result<u32> {
        if name.is_empty() {
            return Ok(NULL_CS);
        }
        if name.len() == 1 {
            return Ok(SINGLE_BASE + u32::from(name[0]));
        }
        self.intern_hashed(name, Self::hash(name))
    }
    pub fn lookup(&self, name: &[u8]) -> Option<u32> {
        if name.is_empty() {
            return Some(NULL_CS);
        }
        if name.len() == 1 {
            return Some(SINGLE_BASE + u32::from(name[0]));
        }
        self.lookup_hashed(name, Self::hash(name))
    }
    pub fn lookup_hashed(&self, name: &[u8], hash: u64) -> Option<u32> {
        let bucket = self.buckets[self.find_slot(name, hash)];
        (bucket != 0).then_some(bucket as u32)
    }
    fn hash(name: &[u8]) -> u64 {
        name.iter()
            .fold(Self::HASH_START, |hash, &byte| Self::hash_byte(hash, byte))
    }
    // Both callers select multi-byte names after the fixed-symbol path. The
    // lexer hashes consumed, translated letters; intern hashes its whole slice.
    #[cfg_attr(feature = "native-profile", inline(never))]
    pub fn intern_hashed(&mut self, name: &[u8], hash: u64) -> Result<u32> {
        let slot = self.find_slot(name, hash);
        let bucket = self.buckets[slot];
        if bucket == 0 {
            self.insert(name, u64::from(hash as u32), slot)
        } else {
            Ok(bucket as u32)
        }
    }
    fn find_slot(&self, name: &[u8], hash: u64) -> usize {
        let mask = self.buckets.len() - 1;
        let mut slot = hash as usize & mask;
        let fragment = u64::from(hash as u32);
        loop {
            let bucket = self.buckets[slot];
            if bucket == 0 {
                return slot;
            }
            if bucket >> 32 == fragment {
                let id = bucket as u32;
                #[cfg(feature = "native-profile")]
                // SAFETY: occupied buckets hold IDs assigned after appending
                // their spelling. Subtracting NAMED selects that entry;
                // rehashing preserves IDs and entries are never removed.
                let spelling = unsafe { self.spellings.get_unchecked(id as usize - NAMED) };
                #[cfg(not(feature = "native-profile"))]
                let spelling = &self.spellings[id as usize - NAMED];
                // SAFETY: occupied buckets refer to initialized immutable
                // ranges retained by bytes. This view ends before insertion.
                if same_spelling(unsafe { spelling.as_ref() }, name) {
                    return slot;
                }
            }
            slot = (slot + 1) & mask;
        }
    }
    #[inline(never)]
    fn insert(&mut self, name: &[u8], fragment: u64, slot: usize) -> Result<u32> {
        if self.len() >= CS as usize {
            return Err(Error::Capacity);
        }
        let id = self.len() as u32;
        let spelling = self.bytes.append(name);
        self.spellings.push(spelling);
        self.buckets[slot] = (fragment << 32) | u64::from(id);
        if self.spellings.len() * 2 > self.buckets.len() {
            let mut buckets = vec![0; self.buckets.len() * 2];
            // With 24-bit name IDs and a half-full table, bucket indices use
            // at most 25 hash bits. The fragment retains all of those bits.
            for &bucket in &self.buckets {
                if bucket == 0 {
                    continue;
                }
                let mut slot = (bucket >> 32) as usize & (buckets.len() - 1);
                while buckets[slot] != 0 {
                    slot = (slot + 1) & (buckets.len() - 1);
                }
                buckets[slot] = bucket;
            }
            self.buckets = buckets;
        }
        Ok(id)
    }
}

// Typical names compare as two overlapping words of equal-length spellings,
// which covers every byte without a library call.
#[inline(always)]
fn same_spelling(a: &[u8], b: &[u8]) -> bool {
    fn words<const N: usize>(a: &[u8], b: &[u8], at: usize) -> bool {
        a[at..at + N] == b[at..at + N]
    }
    let length = a.len();
    if length != b.len() {
        return false;
    }
    match length {
        2..4 => words::<2>(a, b, 0) && words::<2>(a, b, length - 2),
        4..8 => words::<4>(a, b, 0) && words::<4>(a, b, length - 4),
        8..=16 => words::<8>(a, b, 0) && words::<8>(a, b, length - 8),
        _ => a == b,
    }
}

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
