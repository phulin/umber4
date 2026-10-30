use crate::{Error, Result, token::*};
use std::cell::{Cell, RefCell, UnsafeCell};
use std::mem::MaybeUninit;
use std::rc::{Rc, Weak};

const WORDS: usize = 1 << CHUNK_BITS;
const CACHE_CHUNKS: usize = 16;
type Cells = [UnsafeCell<MaybeUninit<Token>>; WORDS];

/// Expected retention selects a bump tail, never the release boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    Durable,
    Transient,
}

#[derive(Default)]
struct Pool {
    active: [RefCell<Option<(PageOwner, usize)>>; 2],
    free: RefCell<Vec<Rc<Page>>>,
    ids: RefCell<Vec<u32>>,
    #[cfg(test)]
    created: Cell<usize>,
}
struct Page {
    cells: Box<Cells>,
    pool: Weak<Pool>,
    class: Cell<Class>,
}
// Active tails and token pieces own these handles. The cache holds bare Rc
// records, so the last checked-out handle can return the complete allocation,
// including its counter. Page -> Pool is weak: neither cache nor tail cycles.
#[derive(Clone)]
struct PageOwner(Rc<Page>);
impl std::ops::Deref for PageOwner {
    type Target = Page;
    fn deref(&self) -> &Page {
        &self.0
    }
}
impl Drop for PageOwner {
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1
            && let Some(pool) = self.pool.upgrade()
        {
            let mut free = pool.free.borrow_mut();
            if free.len() < CACHE_CHUNKS {
                free.push(Rc::clone(&self.0));
            }
        }
    }
}
impl Pool {
    fn rewind(&self, piece: &Piece, count: u32) -> bool {
        let mut active = self.active[piece.page.class.get() as usize].borrow_mut();
        if let Some((page, used)) = active.as_mut()
            && Rc::ptr_eq(&page.0, &piece.page.0)
        {
            let base = UnsafeCell::raw_get(page.cells.as_ptr()).cast::<Token>();
            if base.wrapping_add(*used) == piece.base.wrapping_add(piece.capacity as usize) {
                *used -= count as usize;
                return true;
            }
        }
        false
    }
    fn reserve(self: &Rc<Self>, class: Class, requested: usize, start: u32) -> Piece {
        let mut active = self.active[class as usize].borrow_mut();
        if active.as_ref().is_none_or(|(_, used)| *used == WORDS) {
            // Retire a full tail before looking in the cache, so a completely
            // dead active page can serve this reservation without more backing.
            drop(active.take());
            let page = self.free.borrow_mut().pop().unwrap_or_else(|| {
                #[cfg(test)]
                self.created.set(self.created.get() + 1);
                let cells = Box::<Cells>::new_uninit();
                // SAFETY: UnsafeCell<MaybeUninit<Token>> permits uninitialized
                // contents and has Token's layout. Publication follows writes.
                let cells = unsafe { cells.assume_init() };
                Rc::new(Page {
                    cells,
                    pool: Rc::downgrade(self),
                    class: Cell::new(class),
                })
            });
            // A cached record has no active tail, piece, or published reader.
            debug_assert_eq!(Rc::strong_count(&page), 1);
            page.class.set(class);
            *active = Some((PageOwner(page), 0));
        }
        let (page, used) = active.as_mut().unwrap();
        let capacity = requested.min(WORDS - *used);
        let base = UnsafeCell::raw_get(page.cells.as_ptr()).cast::<Token>();
        // SAFETY: used is inside this page's complete cell allocation.
        let base = unsafe { base.add(*used) };
        let piece = Piece {
            page: page.clone(),
            base,
            start,
            capacity: capacity as u32,
        };
        *used += capacity;
        piece
    }
}

struct Piece {
    page: PageOwner,
    base: *mut Token,
    start: u32,
    capacity: u32,
}
#[derive(Default)]
pub(crate) struct Tokens {
    first: Option<Piece>,
    rest: Vec<Piece>,
    pub len: u32,
}
impl Tokens {
    fn last(&self) -> Option<&Piece> {
        self.rest.last().or(self.first.as_ref())
    }
    fn last_mut(&mut self) -> Option<&mut Piece> {
        self.rest.last_mut().or(self.first.as_mut())
    }
    #[inline]
    fn piece(&self, position: u32) -> &Piece {
        let first = self.first.as_ref().unwrap();
        if position < first.capacity {
            return first;
        }
        let index = self.rest.partition_point(|p| p.start <= position) - 1;
        &self.rest[index]
    }
    #[inline]
    pub fn window(&self, start: u32, end: u32) -> (*const Token, *const Token, u32) {
        assert!(start < end && end <= self.len);
        let piece = self.piece(start);
        let next = end.min(piece.start + piece.capacity);
        // SAFETY: each reservation stays inside its stable boxed page. The
        // published length covers initialized immutable cells only. Owners of
        // this Tokens keep every page alive, including across pool reuse.
        unsafe {
            let first = piece.base.add((start - piece.start) as usize);
            (first, first.add((next - start) as usize), next)
        }
    }
    #[inline]
    pub fn get(&self, position: u32) -> Token {
        assert!(position < self.len);
        // SAFETY: published positions are initialized.
        unsafe { self.read(position) }
    }
    // SAFETY: position must be initialized. Its page is retained by self.
    #[inline]
    unsafe fn read(&self, position: u32) -> Token {
        let piece = self.piece(position);
        unsafe { piece.base.add((position - piece.start) as usize).read() }
    }
    pub fn equal(&self, mut a: u32, other: &Self, mut b: u32, length: u32) -> bool {
        assert!(a <= self.len && length <= self.len - a);
        assert!(b <= other.len && length <= other.len - b);
        let end = a + length;
        while a != end {
            let (first, _, a_end) = self.window(a, end);
            let (second, _, b_end) = other.window(b, b + end - a);
            let count = (a_end - a).min(b_end - b) as usize;
            // SAFETY: both immutable windows contain count initialized words.
            // Token is a transparent u32 with derived equality; comparing the
            // integers lets the slice comparison use bcmp.
            if unsafe {
                std::slice::from_raw_parts(first.cast::<u32>(), count)
                    != std::slice::from_raw_parts(second.cast::<u32>(), count)
            } {
                return false;
            }
            a += count as u32;
            b += count as u32;
        }
        true
    }
}
// The write cursor runs from base, which holds position start, to the
// reservation limit. Contiguous growth keeps base and start. All three
// pointers are null before the first reservation.
pub(crate) struct Builder {
    tokens: Tokens,
    pool: Rc<Pool>,
    class: Class,
    owner: Option<Owner>,
    base: *mut Token,
    next: *mut Token,
    limit: *mut Token,
    start: u32,
}
impl Builder {
    pub fn id(&self) -> u32 {
        self.owner.as_ref().unwrap().id
    }
    pub fn len(&self) -> u32 {
        self.start + Self::words(self.base, self.next)
    }
    fn words(from: *mut Token, to: *mut Token) -> u32 {
        ((to.addr() - from.addr()) / size_of::<Token>()) as u32
    }
    #[inline(always)]
    pub fn push(&mut self, token: Token) -> Result<()> {
        if self.next == self.limit {
            self.grow()?;
        }
        // SAFETY: grow reserves a disjoint unused suffix of a stable page.
        // Each word is initialized before the cursor passes it; the cursor
        // remains inside this reservation or at its limit. Published cells
        // stay immutable, including when other builders share this page.
        unsafe {
            self.next.write(token);
            self.next = self.next.add(1);
        }
        Ok(())
    }
    /// Appends tokens from `next` until it returns None, exactly as repeated
    /// pushes would. The cursor stays local, so writes through it cannot force
    /// the producer's state back to memory between tokens.
    #[inline(always)]
    pub fn extend_with(&mut self, mut next: impl FnMut() -> Option<Token>) -> Result<()> {
        let (mut cursor, mut limit) = (self.next, self.limit);
        while let Some(token) = next() {
            if cursor == limit {
                self.next = cursor;
                self.grow()?;
                (cursor, limit) = (self.next, self.limit);
            }
            // SAFETY: as in push, the cursor stays inside this builder's
            // reservation and initializes each word before passing it.
            unsafe {
                cursor.write(token);
                cursor = cursor.add(1);
            }
        }
        self.next = cursor;
        Ok(())
    }
    /// Appends a run of tokens, growing exactly where pushes would.
    #[inline]
    pub fn extend(&mut self, mut tokens: &[Token]) -> Result<()> {
        while !tokens.is_empty() {
            if self.next == self.limit {
                self.grow()?;
            }
            let count = tokens
                .len()
                .min(Self::words(self.next, self.limit) as usize);
            // SAFETY: count words fit in this builder's unused reservation. The
            // source is initialized elsewhere: published values never overlap an
            // unpublished reservation.
            unsafe {
                std::ptr::copy_nonoverlapping(tokens.as_ptr(), self.next, count);
                self.next = self.next.add(count);
            }
            tokens = &tokens[count..];
        }
        Ok(())
    }
    #[inline(never)]
    fn grow(&mut self) -> Result<()> {
        let len = self.len();
        // Reservations end at u32::MAX positions, so the cursor's position
        // always fits the published length.
        let requested = (len as usize)
            .clamp(64, WORDS)
            .min((u32::MAX - len) as usize);
        if requested == 0 {
            return Err(Error::Capacity);
        }
        let piece = self.pool.reserve(self.class, requested, len);
        self.next = piece.base;
        // SAFETY: the reservation lies inside its page's cell allocation.
        self.limit = unsafe { piece.base.add(piece.capacity as usize) };
        if let Some(last) = self.tokens.last_mut()
            && Rc::ptr_eq(&last.page.0, &piece.page.0)
            && last.base.wrapping_add(last.capacity as usize) == piece.base
        {
            last.capacity += piece.capacity;
            return Ok(());
        }
        self.base = piece.base;
        self.start = len;
        if self.tokens.first.is_none() {
            self.tokens.first = Some(piece);
        } else {
            self.tokens.rest.push(piece);
        }
        Ok(())
    }
    pub fn get(&self, position: u32) -> Token {
        assert!(position < self.len());
        // SAFETY: positions before the cursor are initialized.
        unsafe { self.tokens.read(position) }
    }
    pub fn definition_mut(&mut self) -> &mut Definition {
        let owner = self.owner.as_ref().unwrap();
        // SAFETY: the unpublished builder is the sole external owner and never
        // exposes a Value view. Metadata stays in the slot even on failed builds.
        unsafe { &mut (*owner.slot().value.get()).definition }
    }
    pub fn finish(mut self) -> Owner {
        self.tokens.len = self.len();
        let remaining = Self::words(self.next, self.limit);
        if remaining != 0 {
            let last = self.tokens.last_mut().unwrap();
            if self.pool.rewind(last, remaining) {
                last.capacity -= remaining;
            }
        }
        // SAFETY: only this builder owns the unpublished value. get refuses
        // unpublished slots, and the builder exposes token copies and IDs only.
        // Moving the completed descriptor leaves every token cell in place.
        self.return_storage();
        let owner = self.owner.take().unwrap();
        owner.slot().published.set(true);
        owner
    }
    fn return_storage(&mut self) {
        let owner = self.owner.as_ref().unwrap();
        // SAFETY: an unpublished builder is the sole external owner. No view
        // of this slot's contents is exposed until finish marks it published.
        let value = unsafe { &mut *owner.slot().value.get() };
        value.tokens = std::mem::take(&mut self.tokens);
    }
}
impl Drop for Builder {
    fn drop(&mut self) {
        if self.owner.is_some() {
            // Owner::drop then releases reservations and clears metadata through
            // the same path as a published value, retaining reusable capacity.
            self.return_storage();
        }
    }
}

#[path = "value_headers.rs"]
mod headers;
pub(crate) use headers::{Owner, Value, Values};

#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
