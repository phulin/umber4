//! Small root directories with reusable, independently owned COW pages.
use crate::{token::Binding, value_metadata::Buffer, values::Owner};
use std::{
    cell::RefCell,
    ops::{Index, IndexMut},
    rc::Rc,
};

const ENTRIES: usize = 64;
const CACHED: usize = 16;

struct Pool<T>(RefCell<Vec<Rc<[T; ENTRIES]>>>);
struct Page<T: Clone + Default> {
    data: Option<Rc<[T; ENTRIES]>>,
    pool: Rc<Pool<T>>,
}
impl<T: Clone + Default> Clone for Page<T> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            pool: self.pool.clone(),
        }
    }
}
impl<T: Clone + Default> Drop for Page<T> {
    fn drop(&mut self) {
        let mut data = self.data.take().unwrap();
        if let Some(entries) = Rc::get_mut(&mut data) {
            // A cached block must not retain abandoned document payloads.
            for entry in entries {
                *entry = T::default();
            }
            let mut free = self.pool.0.borrow_mut();
            if free.len() < CACHED {
                free.push(data);
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Table<T: Clone + Default> {
    pages: Buffer<Page<T>>,
    pool: Rc<Pool<T>>,
    default: T,
    len: usize,
}
impl<T: Clone + Default> Table<T> {
    pub fn new(len: usize) -> Self {
        let mut table = Self {
            pages: Buffer::new(&Rc::default()),
            pool: Rc::new(Pool(RefCell::new(Vec::new()))),
            default: T::default(),
            len: 0,
        };
        table.grow(len);
        table
    }
    fn block(&self) -> Rc<[T; ENTRIES]> {
        self.pool
            .0
            .borrow_mut()
            .pop()
            .unwrap_or_else(|| Rc::new(std::array::from_fn(|_| T::default())))
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn grow(&mut self, len: usize) {
        while self.pages.len() * ENTRIES < len {
            self.pages.push(Page {
                data: Some(self.block()),
                pool: self.pool.clone(),
            });
        }
        self.len = self.len.max(len);
    }
    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.pages
            .iter()
            .flat_map(|p| p.data.as_ref().unwrap().iter())
            .take(self.len)
    }
}
impl<T: Clone + Default> Index<usize> for Table<T> {
    type Output = T;
    #[inline]
    fn index(&self, index: usize) -> &T {
        if index >= self.len {
            return &self.default;
        }
        &self.pages[index / ENTRIES].data.as_ref().unwrap()[index % ENTRIES]
    }
}
impl<T: Clone + Default> IndexMut<usize> for Table<T> {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut T {
        if index >= self.len {
            self.grow(index + 1);
        }
        let page = index / ENTRIES;
        if Rc::strong_count(self.pages[page].data.as_ref().unwrap()) != 1 {
            let mut copy = self.block();
            let entries = Rc::get_mut(&mut copy).unwrap();
            entries.clone_from_slice(self.pages[page].data.as_ref().unwrap().as_slice());
            self.pages[page].data = Some(copy);
        }
        &mut Rc::get_mut(self.pages[page].data.as_mut().unwrap()).unwrap()[index % ENTRIES]
    }
}

#[derive(Clone, Default)]
struct Entry {
    binding: Binding,
    owner: Option<Owner>,
}
#[derive(Clone)]
pub(crate) struct Bindings(Table<Entry>);
impl Bindings {
    pub fn new(len: usize) -> Self {
        Self(Table::new(len))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn grow(&mut self, len: usize) {
        self.0.grow(len);
    }
    pub fn replace_owner(&mut self, id: usize, owner: Option<Owner>) -> Option<Owner> {
        std::mem::replace(&mut self.0[id].owner, owner)
    }
    pub fn owner(&self, id: usize) -> Option<&Owner> {
        self.0[id].owner.as_ref()
    }
    #[cfg(test)]
    pub fn owners(&self) -> impl Iterator<Item = &Owner> {
        self.0.iter().filter_map(|e| e.owner.as_ref())
    }
}
impl Index<usize> for Bindings {
    type Output = Binding;
    #[inline]
    fn index(&self, index: usize) -> &Binding {
        &self.0[index].binding
    }
}
impl IndexMut<usize> for Bindings {
    #[inline]
    fn index_mut(&mut self, index: usize) -> &mut Binding {
        &mut self.0[index].binding
    }
}

#[cfg(test)]
#[path = "tables_tests.rs"]
mod tests;
