use crate::{Error, Result, token::*};
use std::cell::{Cell, RefCell, UnsafeCell};
use std::mem::MaybeUninit;
use std::rc::{Rc, Weak};

const WORDS: usize = 1 << CHUNK_BITS;
const CACHE_CHUNKS: usize = 16;
type Cells = [UnsafeCell<MaybeUninit<Token>>; WORDS];

#[derive(Default)]
struct Pool {
    active: RefCell<Option<(Rc<Page>, usize)>>,
    free: RefCell<Vec<Box<Cells>>>,
    ids: RefCell<Vec<u32>>,
}
struct Page {
    cells: Option<Box<Cells>>,
    pool: Weak<Pool>,
}
impl Drop for Page {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.upgrade() {
            let mut free = pool.free.borrow_mut();
            if free.len() < CACHE_CHUNKS {
                free.push(self.cells.take().unwrap());
            }
        }
    }
}
impl Pool {
    fn rewind(&self, piece: &Piece, count: u32) -> bool {
        let mut active = self.active.borrow_mut();
        if let Some((page, used)) = active.as_mut()
            && Rc::ptr_eq(page, &piece.page)
        {
            let cells = page.cells.as_ref().unwrap();
            let base = UnsafeCell::raw_get(cells.as_ptr()).cast::<Token>();
            if base.wrapping_add(*used) == piece.base.wrapping_add(piece.capacity as usize) {
                *used -= count as usize;
                return true;
            }
        }
        false
    }
    fn reserve(self: &Rc<Self>, requested: usize, start: u32) -> Piece {
        let mut active = self.active.borrow_mut();
        if active.as_ref().is_none_or(|(_, used)| *used == WORDS) {
            let cells = self.free.borrow_mut().pop().unwrap_or_else(|| {
                let cells = Box::<Cells>::new_uninit();
                // SAFETY: UnsafeCell<MaybeUninit<Token>> permits uninitialized
                // contents and has Token's layout. Publication follows writes.
                unsafe { cells.assume_init() }
            });
            *active = Some((
                Rc::new(Page {
                    cells: Some(cells),
                    pool: Rc::downgrade(self),
                }),
                0,
            ));
        }
        let (page, used) = active.as_mut().unwrap();
        let capacity = requested.min(WORDS - *used);
        let cells = page.cells.as_ref().unwrap();
        let base = UnsafeCell::raw_get(cells.as_ptr()).cast::<Token>();
        // SAFETY: used is inside this page's complete cell allocation.
        let base = unsafe { base.add(*used) };
        let piece = Piece {
            page: Rc::clone(page),
            base,
            start,
            capacity: capacity as u32,
        };
        *used += capacity;
        piece
    }
}

struct Piece {
    page: Rc<Page>,
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
        let piece = self.piece(position);
        // SAFETY: position is initialized and its page is retained by this value.
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
            if unsafe {
                std::slice::from_raw_parts(first, count)
                    != std::slice::from_raw_parts(second, count)
            } {
                return false;
            }
            a += count as u32;
            b += count as u32;
        }
        true
    }
}
pub(crate) struct Builder {
    tokens: Tokens,
    pool: Rc<Pool>,
    owner: Owner,
    next: *mut Token,
    remaining: u32,
}
impl Builder {
    pub fn id(&self) -> u32 {
        self.owner.id
    }
    pub fn len(&self) -> u32 {
        self.tokens.len
    }
    #[inline(always)]
    pub fn push(&mut self, token: Token) -> Result<()> {
        if self.tokens.len == u32::MAX {
            return Err(Error::Capacity);
        }
        if self.remaining == 0 {
            self.grow();
        }
        // SAFETY: grow reserves a disjoint unused suffix of a stable page.
        // Each word is initialized before len advances; the cursor remains
        // inside this reservation or one past its end. Published cells stay
        // immutable, including when other builders share this page.
        unsafe {
            self.next.write(token);
            self.next = self.next.add(1);
        }
        self.remaining -= 1;
        self.tokens.len += 1;
        Ok(())
    }
    #[inline(never)]
    fn grow(&mut self) {
        let requested = (self.tokens.len as usize)
            .clamp(64, WORDS)
            .min((u32::MAX - self.tokens.len) as usize);
        let piece = self.pool.reserve(requested, self.tokens.len);
        self.next = piece.base;
        self.remaining = piece.capacity;
        if let Some(last) = self.tokens.rest.last_mut().or(self.tokens.first.as_mut()) {
            if Rc::ptr_eq(&last.page, &piece.page)
                && last.base.wrapping_add(last.capacity as usize) == piece.base
            {
                last.capacity += piece.capacity;
                return;
            }
            self.tokens.rest.push(piece);
        } else {
            self.tokens.first = Some(piece);
        }
    }
    pub fn get(&self, position: u32) -> Token {
        self.tokens.get(position)
    }
    pub fn finish(mut self, definition: Option<Definition>) -> Owner {
        if self.remaining != 0 {
            let last = self
                .tokens
                .rest
                .last_mut()
                .or(self.tokens.first.as_mut())
                .unwrap();
            if self.pool.rewind(last, self.remaining) {
                last.capacity -= self.remaining;
            }
        }
        // SAFETY: only this builder owns the unpublished value. get refuses
        // unpublished slots, and the builder exposes token copies and IDs only.
        // Moving the completed descriptor leaves every token cell in place.
        let value = unsafe { &mut *self.owner.0.value.get() };
        value.tokens = self.tokens;
        value.definition = definition;
        self.owner.0.published.set(true);
        self.owner
    }
}

pub(crate) struct Value {
    pub id: u32,
    pub tokens: Tokens,
    definition: Option<Definition>,
}
impl Value {
    #[inline]
    pub fn definition(&self) -> &Definition {
        self.definition.as_ref().unwrap()
    }
}
struct Slot {
    value: UnsafeCell<Value>,
    published: Cell<bool>,
    registered: Cell<bool>,
    pool: Weak<Pool>,
}

// The registry owns one reference per reusable header. External owners retain
// initialized contents; only an unpublished Builder can change those contents.
#[derive(Clone)]
pub(crate) struct Owner(Rc<Slot>);
impl std::ops::Deref for Owner {
    type Target = Value;
    fn deref(&self) -> &Value {
        // SAFETY: Owners are created after initialization. A returned view
        // borrows this owner, so the last owner cannot disappear during its use.
        unsafe { &*self.0.value.get() }
    }
}
impl Drop for Owner {
    #[inline]
    fn drop(&mut self) {
        if Rc::strong_count(&self.0) == 1 + usize::from(self.0.registered.get()) {
            self.0.release();
        }
    }
}
impl Slot {
    #[inline(never)]
    fn release(&self) {
        self.published.set(false);
        // SAFETY: called only by the last external owner. All borrowed views
        // have ended, and registered headers retain no reader of their contents.
        let value = unsafe { &mut *self.value.get() };
        let tokens = std::mem::take(&mut value.tokens);
        value.definition = None;
        if let Some(pool) = self.pool.upgrade() {
            if let Some(last) = tokens.rest.last().or(tokens.first.as_ref()) {
                // No reader remains. Reuse the whole final reservation only
                // if later reservations have already left the bump tail.
                pool.rewind(last, last.capacity);
            }
            pool.ids.borrow_mut().push(value.id);
        }
    }
}

#[derive(Default)]
pub(crate) struct Values {
    pool: Rc<Pool>,
    slots: Vec<Rc<Slot>>,
}
impl Values {
    pub fn builder(&mut self) -> Result<Builder> {
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
            self.slots.push(Rc::new(Slot {
                value: UnsafeCell::new(Value {
                    id,
                    tokens: Tokens::default(),
                    definition: None,
                }),
                published: Cell::new(false),
                registered: Cell::new(true),
                pool: Rc::downgrade(&self.pool),
            }));
        }
        // Free IDs name cleared headers with only the registry reference.
        let slot = Rc::clone(&self.slots[id as usize]);
        Ok(Builder {
            tokens: Tokens::default(),
            pool: Rc::clone(&self.pool),
            owner: Owner(slot),
            next: std::ptr::null_mut(),
            remaining: 0,
        })
    }
    #[inline]
    pub fn get(&self, id: u32) -> Owner {
        let slot = &self.slots[id as usize];
        assert!(slot.published.get(), "retained published token value");
        Owner(Rc::clone(slot))
    }
    // Used only by the non-reentrant copy/window queries below. Their temporary
    // view never escapes or runs code that could release an external owner.
    fn tokens(&self, id: u32) -> &Tokens {
        let slot = &self.slots[id as usize];
        assert!(slot.published.get(), "retained published token value");
        // SAFETY: publication initializes the value, and no owner can disappear
        // during these queries. Returned raw windows need a caller-held owner.
        unsafe { &(*slot.value.get()).tokens }
    }
    pub fn token(&self, id: u32, position: u32) -> Token {
        self.tokens(id).get(position)
    }
    pub fn window(&self, span: Span, start: u32) -> (*const Token, *const Token, u32) {
        self.tokens(span.store).window(start, span.end)
    }
}

impl Drop for Values {
    fn drop(&mut self) {
        for slot in &self.slots {
            slot.registered.set(false);
        }
    }
}

#[cfg(test)]
impl Values {
    pub fn retained(&self) -> (usize, usize, usize) {
        let live = self
            .slots
            .iter()
            .filter(|slot| slot.published.get())
            .count();
        (live, self.slots.len(), self.pool.free.borrow().len())
    }
}
#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
