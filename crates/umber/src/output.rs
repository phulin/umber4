//! Detached records and bounded append chunks. No record names a live TeX node.
use crate::{
    tfm::{FontResource, ScaledFont},
    value_metadata::{Buffer, Store},
};
use std::{
    cell::{Cell, OnceCell},
    ops::Range,
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
/// Ordered immutable page output. Glyph ranges index the page's glyph bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutputItem {
    GlyphRun {
        font: u32,
        x: i32,
        y: i32,
        glyphs: Range<u32>,
    },
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
    items: Buffer<OutputItem>,
    glyphs: Buffer<u8>,
    placement_count: u32,
    fonts: Buffer<FontResource>,
}
impl Page {
    pub fn boxes(&self) -> &[PackedBox] {
        &self.boxes
    }
    /// Reconstruct owned absolute placements in shipout order.
    /// Each traversal reads retained font widths; no position array is cached.
    pub fn placements(&self) -> Placements<'_> {
        Placements {
            items: self.items.iter(),
            glyphs: &self.glyphs,
            fonts: &self.fonts,
            run: None,
            remaining: self.placement_count as usize,
        }
    }
    pub fn items(&self) -> &[OutputItem] {
        &self.items
    }
    pub fn glyphs(&self) -> &[u8] {
        &self.glyphs
    }
    pub fn fonts(&self) -> &[FontResource] {
        &self.fonts
    }
    pub(crate) fn new(pools: &Pools, counts: [i32; 256], output_penalty: i32) -> Self {
        Self {
            counts,
            output_penalty,
            boxes: Buffer::new(&pools.boxes),
            items: Buffer::new(&pools.items),
            glyphs: Buffer::new(&pools.glyphs),
            placement_count: 0,
            fonts: Buffer::new(&pools.fonts),
        }
    }
    pub(crate) fn box_record(&mut self, record: PackedBox) -> u32 {
        let id = u32::try_from(self.boxes.len()).expect("page box capacity");
        self.boxes.push(record);
        id
    }
    fn count_placement(&mut self) {
        // Match the previous single placement buffer's aggregate u32 bound,
        // even when glyphs and rules now occupy different backing buffers.
        self.placement_count = self
            .placement_count
            .checked_add(1)
            .expect("metadata capacity");
    }
    pub(crate) fn rule(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.count_placement();
        self.items.push(OutputItem::Rule {
            x,
            y,
            width,
            height,
        });
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
struct Glyphs<'a> {
    characters: std::slice::Iter<'a, u8>,
    metrics: &'a ScaledFont,
    font: u32,
    x: i32,
    y: i32,
}
/// A forward iterator of owned positions backed by immutable page/font data.
pub struct Placements<'a> {
    items: std::slice::Iter<'a, OutputItem>,
    glyphs: &'a [u8],
    fonts: &'a [FontResource],
    run: Option<Glyphs<'a>>,
    remaining: usize,
}
impl Iterator for Placements<'_> {
    type Item = Placement;
    fn next(&mut self) -> Option<Placement> {
        loop {
            if let Some(run) = &mut self.run {
                if let Some(&character) = run.characters.next() {
                    let placement = Placement::Glyph {
                        font: run.font,
                        character,
                        x: run.x,
                        y: run.y,
                    };
                    if run.characters.len() != 0 {
                        // Lowering checked every character before publication.
                        let width = run
                            .metrics
                            .metrics(character)
                            .expect("published glyph")
                            .width
                            .0;
                        run.x = run.x.wrapping_add(width);
                    }
                    self.remaining -= 1;
                    return Some(placement);
                }
                self.run = None;
            }
            match self.items.next()? {
                OutputItem::GlyphRun { font, x, y, glyphs } => {
                    self.run = Some(Glyphs {
                        characters: self.glyphs[glyphs.start as usize..glyphs.end as usize].iter(),
                        metrics: self.fonts[*font as usize].descriptor(),
                        font: *font,
                        x: *x,
                        y: *y,
                    });
                }
                OutputItem::Rule {
                    x,
                    y,
                    width,
                    height,
                } => {
                    self.remaining -= 1;
                    return Some(Placement::Rule {
                        x: *x,
                        y: *y,
                        width: *width,
                        height: *height,
                    });
                }
            }
        }
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}
impl ExactSizeIterator for Placements<'_> {}
impl std::iter::FusedIterator for Placements<'_> {}

#[derive(Default)]
pub(crate) struct Runs {
    pending: Option<(u32, i32, i32, u32)>,
}
impl Runs {
    pub(crate) fn finish(&mut self, page: &mut Page) {
        if let Some((font, x, y, start)) = self.pending.take() {
            page.items.push(OutputItem::GlyphRun {
                font,
                x,
                y,
                glyphs: start..page.glyphs.len() as u32,
            });
        }
    }
    pub(crate) fn glyph(&mut self, page: &mut Page, font: u32, character: u8, x: i32, y: i32) {
        page.count_placement();
        if self.pending.is_some_and(|run| run.0 != font) {
            self.finish(page);
        }
        self.pending
            .get_or_insert((font, x, y, page.glyphs.len() as u32));
        page.glyphs.push(character);
    }
}
#[derive(Default)]
pub(crate) struct Pools {
    boxes: Rc<Store<PackedBox>>,
    items: Rc<Store<OutputItem>>,
    glyphs: Rc<Store<u8>>,
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

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;
