//! Detached records and bounded append chunks. No record names a live TeX node.
use crate::{
    tfm::FontResource,
    value_metadata::{Buffer, Store},
};
use std::{
    cell::{Cell, OnceCell},
    rc::Rc,
};
const PAGES: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GlueSetting {
    /// 0 normal, 1 stretching, -1 shrinking.
    pub sign: i8,
    /// 0 normal, 1 fil, 2 fill, 3 filll.
    pub order: u8,
    pub ratio: f64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PackedBox {
    pub parent: Option<u32>,
    pub vertical: bool,
    /// Reference point; y is its baseline, in scaled points from the page top.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub glue: GlueSetting,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    Glyph {
        font: u32,
        character: u8,
        x: i32,
        y: i32,
    },
    /// Top-left corner and positive dimensions, all in scaled points.
    Rule {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
}
pub struct Page {
    pub counts: [i32; 256],
    pub output_penalty: i32,
    boxes: Buffer<PackedBox>,
    placements: Buffer<Placement>,
    fonts: Buffer<FontResource>,
}
impl Page {
    pub fn boxes(&self) -> &[PackedBox] {
        &self.boxes
    }
    pub fn placements(&self) -> &[Placement] {
        &self.placements
    }
    pub fn fonts(&self) -> &[FontResource] {
        &self.fonts
    }
    pub(crate) fn new(pools: &Pools, counts: [i32; 256], output_penalty: i32) -> Self {
        Self {
            counts,
            output_penalty,
            boxes: Buffer::new(&pools.boxes),
            placements: Buffer::new(&pools.placements),
            fonts: Buffer::new(&pools.fonts),
        }
    }
    pub(crate) fn box_record(&mut self, record: PackedBox) -> u32 {
        let id = u32::try_from(self.boxes.len()).expect("page box capacity");
        self.boxes.push(record);
        id
    }
    pub(crate) fn place(&mut self, record: Placement) {
        self.placements.push(record);
    }
    pub(crate) fn font(&mut self, resource: &FontResource) -> u32 {
        if let Some(i) = self.fonts.iter().position(|f| f.same(resource)) {
            return i as u32;
        }
        let i = self.fonts.len() as u32;
        self.fonts.push(resource.clone());
        i
    }
}
#[derive(Default)]
pub(crate) struct Pools {
    boxes: Rc<Store<PackedBox>>,
    placements: Rc<Store<Placement>>,
    fonts: Rc<Store<FontResource>>,
}
struct Chunk {
    previous: Pages,
    pages: [OnceCell<Page>; PAGES],
    initialized: Cell<usize>,
}
/// A replacement immutable page prefix. Cloning retains a root and endpoint.
/// Later appends never become visible through an older result or checkpoint.
#[derive(Clone, Default)]
pub struct Pages {
    tail: Option<Rc<Chunk>>,
    endpoint: usize,
    len: usize,
}
impl Pages {
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn get(&self, index: usize) -> Option<&Page> {
        if index >= self.len {
            return None;
        }
        let mut root = self;
        loop {
            let chunk = root.tail.as_ref()?;
            let start = root.len - root.endpoint;
            if index >= start {
                return chunk.pages[index - start].get();
            }
            root = &chunk.previous;
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &Page> {
        let mut chunks = Vec::new();
        let mut root = self;
        while let Some(chunk) = &root.tail {
            chunks.push((&chunk.pages, root.endpoint));
            root = &chunk.previous;
        }
        chunks
            .into_iter()
            .rev()
            .flat_map(|(pages, end)| pages[..end].iter().map(|p| p.get().unwrap()))
    }
    pub(crate) fn push(&mut self, page: Page) {
        if self
            .tail
            .as_ref()
            .is_none_or(|c| self.endpoint != c.initialized.get() || self.endpoint == PAGES)
        {
            self.tail = Some(Rc::new(Chunk {
                previous: self.clone(),
                pages: std::array::from_fn(|_| OnceCell::new()),
                initialized: Cell::new(0),
            }));
            self.endpoint = 0;
        }
        let chunk = self.tail.as_ref().unwrap();
        assert!(chunk.pages[self.endpoint].set(page).is_ok());
        self.endpoint += 1;
        chunk.initialized.set(self.endpoint);
        self.len += 1;
    }
}
impl Drop for Chunk {
    fn drop(&mut self) {
        // Release long unique histories iteratively, without a recursive Rc chain.
        let mut previous = std::mem::take(&mut self.previous);
        while let Some(tail) = previous.tail.take() {
            match Rc::try_unwrap(tail) {
                Ok(mut chunk) => previous = std::mem::take(&mut chunk.previous),
                Err(_) => break,
            }
        }
    }
}
