use crate::{Error, Result, token::*};
use std::cell::{RefCell, UnsafeCell};
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
    next: *mut Token,
    remaining: u32,
}
impl Builder {
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
    fn finish(mut self) -> Tokens {
        if self.remaining != 0 {
            let last = self
                .tokens
                .rest
                .last_mut()
                .or(self.tokens.first.as_mut())
                .unwrap();
            let mut active = self.pool.active.borrow_mut();
            if let Some((page, used)) = active.as_mut()
                && Rc::ptr_eq(page, &last.page)
            {
                let cells = page.cells.as_ref().unwrap();
                let base = UnsafeCell::raw_get(cells.as_ptr()).cast::<Token>();
                if base.wrapping_add(*used) == last.base.wrapping_add(last.capacity as usize) {
                    // This reservation is still the bump tail. Only its unused
                    // suffix is returned; every initialized word stays owned
                    // and immutable. Interleaved later reservations prevent
                    // this rewind, so another builder's cells remain intact.
                    *used -= self.remaining as usize;
                    last.capacity -= self.remaining;
                }
            }
        }
        self.tokens
    }
}

pub(crate) struct Value {
    pub id: u32,
    pub tokens: Tokens,
    definition: Option<Definition>,
    pool: Rc<Pool>,
}
impl Value {
    #[inline]
    pub fn definition(&self) -> &Definition {
        self.definition.as_ref().unwrap()
    }
    pub fn span(&self) -> Span {
        Span {
            store: DURABLE | self.id,
            start: 0,
            end: self.tokens.len,
        }
    }
}
impl Drop for Value {
    fn drop(&mut self) {
        self.pool.ids.borrow_mut().push(self.id);
    }
}
pub(crate) type Owner = Rc<Value>;

#[derive(Default)]
pub(crate) struct Values {
    pool: Rc<Pool>,
    slots: Vec<Weak<Value>>,
}
impl Values {
    pub fn builder(&self) -> Builder {
        Builder {
            tokens: Tokens::default(),
            pool: Rc::clone(&self.pool),
            next: std::ptr::null_mut(),
            remaining: 0,
        }
    }
    pub fn publish(&mut self, builder: Builder, definition: Option<Definition>) -> Result<Owner> {
        assert!(Rc::ptr_eq(&builder.pool, &self.pool));
        let id = self
            .pool
            .ids
            .borrow_mut()
            .pop()
            .unwrap_or(self.slots.len() as u32);
        if id >= SINGLE - DURABLE {
            return Err(Error::Capacity);
        }
        let value = Rc::new(Value {
            id,
            tokens: builder.finish(),
            definition,
            pool: Rc::clone(&self.pool),
        });
        if id as usize == self.slots.len() {
            self.slots.push(Rc::downgrade(&value));
        } else {
            self.slots[id as usize] = Rc::downgrade(&value);
        }
        Ok(value)
    }
    #[inline]
    pub fn get(&self, id: u32) -> Owner {
        self.slots[id as usize]
            .upgrade()
            .expect("retained token value")
    }
}

#[cfg(test)]
impl Values {
    pub fn retained(&self) -> (usize, usize, usize) {
        let live = self
            .slots
            .iter()
            .filter(|slot| slot.strong_count() != 0)
            .count();
        (live, self.slots.len(), self.pool.free.borrow().len())
    }
}
#[cfg(test)]
#[path = "values_tests.rs"]
mod tests;
