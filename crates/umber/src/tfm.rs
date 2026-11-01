//! Validated immutable TFM resources and exact TeX metric scaling.
use crate::{
    Error, Result,
    numeric::Scaled,
    value_metadata::{Buffer, Store},
};
use std::{
    cell::{Cell, OnceCell},
    rc::Rc,
};

#[derive(Clone, Copy, Default, Debug)]
pub(crate) struct CharInfo {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
    pub italic: usize,
    pub tag: u8,
    pub remainder: usize,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(crate) struct Metrics {
    pub width: Scaled,
    pub height: Scaled,
    pub depth: Scaled,
    pub italic: Scaled,
}
pub(crate) struct Resource {
    pub name: Buffer<u8>,
    bytes: Buffer<u8>,
    pub chars: Buffer<CharInfo>,
    pub program: Buffer<[u8; 4]>,
    pub bc: usize,
    pub ec: usize,
    pub design: Scaled,
    pub boundary: u16,
    pub false_boundary: u16,
    pub boundary_label: Option<usize>,
    pub nw: usize,
    pub nh: usize,
    pub nd: usize,
    pub nk: usize,
    pub np: usize,
    metrics: usize,
    kerns: usize,
    params: usize,
}
pub(crate) struct ScaledFont {
    pub resource: usize,
    pub size: Scaled,
    first: usize,
    present: [u64; 4],
    metrics: Buffer<Metrics>,
    kerns: Buffer<Scaled>,
    pub params: Buffer<Scaled>,
}
impl ScaledFont {
    pub fn kern(&self, index: usize) -> Scaled {
        self.kerns[index]
    }
    pub fn exists(&self, character: u8) -> bool {
        self.present[character as usize / 64] & (1u64 << (character % 64)) != 0
    }
    pub fn metrics(&self, character: u8) -> Option<Metrics> {
        self.exists(character)
            .then(|| self.metrics[character as usize - self.first])
    }
    pub fn metric_fields(&self, character: u8) -> Option<Metrics> {
        self.metrics
            .get((character as usize).checked_sub(self.first)?)
            .copied()
    }
}
const FONT_SLOTS: usize = 16;
// The allocation owns many immutable headers; payloads retain their typed pools.
// OnceCell permits appending disjoint slots after older handles are published.
struct ScaledChunk {
    resources: Rc<Buffer<Resource>>,
    slots: [OnceCell<ScaledFont>; FONT_SLOTS],
    len: Cell<usize>,
}
/// An immutable metric version retained independently of its originating engine.
/// A handle retains its scaled-header chunk and the complete registered TFM set.
#[derive(Clone)]
pub struct FontResource {
    chunk: Rc<ScaledChunk>,
    slot: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlyphMetrics {
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub italic: i32,
}
impl FontResource {
    pub(crate) fn descriptor(&self) -> &ScaledFont {
        self.chunk.slots[self.slot].get().expect("published font")
    }
    pub(crate) fn source(&self) -> &Resource {
        &self.chunk.resources[self.descriptor().resource]
    }
    pub fn name(&self) -> &[u8] {
        &self.source().name
    }
    pub fn size(&self) -> i32 {
        self.descriptor().size.0
    }
    pub fn tfm_bytes(&self) -> &[u8] {
        &self.source().bytes
    }
    pub fn design_size(&self) -> i32 {
        self.source().design.0
    }
    pub fn metrics(&self, character: u8) -> Option<GlyphMetrics> {
        let m = self.descriptor().metrics(character)?;
        Some(GlyphMetrics {
            width: m.width.0,
            height: m.height.0,
            depth: m.depth.0,
            italic: m.italic.0,
        })
    }
    pub(crate) fn same(&self, other: &Self) -> bool {
        self.slot == other.slot && Rc::ptr_eq(&self.chunk, &other.chunk)
    }
}
#[derive(Default)]
pub(crate) struct Catalog {
    pub resources: Rc<Buffer<Resource>>,
    pub scaled: Buffer<FontResource>,
    active: Option<Rc<ScaledChunk>>,
    resource_store: Rc<Store<Resource>>,
    scaled_store: Rc<Store<FontResource>>,
    bytes: Rc<Store<u8>>,
    chars: Rc<Store<CharInfo>>,
    instructions: Rc<Store<[u8; 4]>>,
    metrics: Rc<Store<Metrics>>,
    dimensions: Rc<Store<Scaled>>,
    pub sealed: bool,
}
impl Catalog {
    pub fn normalized(name: &[u8]) -> &[u8] {
        name.strip_suffix(b".tfm").unwrap_or(name)
    }
    pub fn find(&self, name: &[u8]) -> Option<usize> {
        let name = Self::normalized(name);
        self.resources.iter().position(|r| r.name.as_ref() == name)
    }
    pub fn register(&mut self, name: &[u8], bytes: &[u8]) -> Result<()> {
        if self.sealed {
            return Err(Error::Resource);
        }
        let name = Self::normalized(name);
        if name.is_empty() || self.find(name).is_some() {
            return Err(Error::Resource);
        }
        let resource = self.parse(name, bytes)?;
        if self.resources.is_empty() {
            self.resources = Rc::new(Buffer::new(&self.resource_store));
        }
        Rc::get_mut(&mut self.resources)
            .ok_or(Error::Resource)?
            .push(resource);
        Ok(())
    }
    fn parse(&self, name: &[u8], bytes: &[u8]) -> Result<Resource> {
        let bad = Error::Font;
        if bytes.len() < 24 {
            return Err(bad);
        }
        let mut h = [0usize; 12];
        for (i, slot) in h.iter_mut().enumerate() {
            *slot = usize::from(u16::from_be_bytes([bytes[2 * i], bytes[2 * i + 1]]));
            if *slot > 32767 {
                return Err(bad);
            }
        }
        let [lf, lh, mut bc, mut ec, nw, nh, nd, ni, nl, nk, ne, np] = h;
        if bc > ec + 1 || ec > 255 || lh < 2 || nw == 0 || nh == 0 || nd == 0 || ni == 0 {
            return Err(bad);
        }
        if bc == 256 {
            bc = 1;
            ec = 0;
        }
        let count = (ec + 1) - bc;
        if lf != 6 + lh + count + nw + nh + nd + ni + nl + nk + ne + np || bytes.len() < lf * 4 {
            return Err(bad);
        }
        let design = Scaled((i32::from_be_bytes(bytes[28..32].try_into().unwrap())) >> 4);
        if design.0 < 65536 {
            return Err(bad);
        }
        let mut chars = Buffer::new(&self.chars);
        for c in bytes[(6 + lh) * 4..(6 + lh + count) * 4].as_chunks::<4>().0 {
            let info = CharInfo {
                width: c[0] as usize,
                height: (c[1] >> 4) as usize,
                depth: (c[1] & 15) as usize,
                italic: (c[2] >> 2) as usize,
                tag: c[2] & 3,
                remainder: c[3] as usize,
            };
            if info.width >= nw
                || info.height >= nh
                || info.depth >= nd
                || info.italic >= ni
                || (info.tag == 1 && info.remainder >= nl)
                || (info.tag == 3 && info.remainder >= ne)
                || (info.tag == 2 && !(bc..=ec).contains(&info.remainder))
            {
                return Err(bad);
            }
            chars.push(info);
        }
        for (i, info) in chars.iter().enumerate() {
            if info.tag != 2 {
                continue;
            }
            let mut next = info.remainder;
            while next < i + bc {
                let q = chars[next - bc];
                if q.tag != 2 {
                    break;
                }
                next = q.remainder;
            }
            if next == i + bc {
                return Err(bad);
            }
        }
        let metrics = 6 + lh + count;
        let lig = metrics + nw + nh + nd + ni;
        let kerns = lig + nl;
        let ext = kerns + nk;
        let params = ext + ne;
        for raw in bytes[metrics * 4..lig * 4].as_chunks::<4>().0 {
            validate_fix(raw)?;
        }
        let exists = |c: usize| c >= bc && c <= ec && chars[c - bc].width != 0;
        let mut program = Buffer::new(&self.instructions);
        let mut boundary = 256u16;
        let mut boundary_label = None;
        for (i, raw) in bytes[lig * 4..kerns * 4]
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
        {
            let [skip, next, op, rem]: [u8; 4] = *raw;
            if skip > 128 {
                if 256 * op as usize + rem as usize >= nl {
                    return Err(bad);
                }
                if skip == 255 && i == 0 {
                    boundary = next as u16;
                }
            } else {
                if next as u16 != boundary && !exists(next as usize) {
                    return Err(bad);
                }
                if op < 128 {
                    if !exists(rem as usize) {
                        return Err(bad);
                    }
                } else if 256 * (op as usize - 128) + rem as usize >= nk {
                    return Err(bad);
                }
                if skip < 128 && i + skip as usize + 1 >= nl {
                    return Err(bad);
                }
            }
            if i + 1 == nl && skip == 255 {
                boundary_label = Some(256 * op as usize + rem as usize);
            }
            program.push([skip, next, op, rem]);
        }
        for raw in bytes[kerns * 4..ext * 4].as_chunks::<4>().0 {
            validate_fix(raw)?;
        }
        for raw in bytes[ext * 4..params * 4].as_chunks::<4>().0 {
            if raw[..3].iter().any(|&c| c != 0 && !exists(c as usize)) || !exists(raw[3] as usize) {
                return Err(bad);
            }
        }
        for raw in bytes[(params + usize::from(np > 0)) * 4..(params + np) * 4]
            .as_chunks::<4>()
            .0
        {
            validate_fix(raw)?;
        }
        let false_boundary = if boundary < 256 && exists(boundary as usize) {
            256
        } else {
            boundary
        };
        let mut saved_name = Buffer::new(&self.bytes);
        saved_name.extend_from_slice(name);
        let mut saved_bytes = Buffer::new(&self.bytes);
        saved_bytes.extend_from_slice(bytes);
        Ok(Resource {
            name: saved_name,
            bytes: saved_bytes,
            chars,
            program,
            bc,
            ec,
            design,
            boundary,
            false_boundary,
            boundary_label,
            nw,
            nh,
            nd,
            nk,
            np,
            metrics,
            kerns,
            params,
        })
    }
    pub fn scaled(&mut self, resource: usize, size: Scaled) -> Result<usize> {
        if let Some(i) = self
            .scaled
            .iter()
            .position(|f| f.descriptor().resource == resource && f.descriptor().size == size)
        {
            return Ok(i);
        }
        let r = &self.resources[resource];
        let dimension = |index| {
            let start = (r.metrics + index) * 4;
            scale(r.bytes[start..start + 4].try_into().unwrap(), size)
        };
        for index in [0, r.nw, r.nw + r.nh, r.nw + r.nh + r.nd] {
            if dimension(index)? != Scaled::ZERO {
                return Err(Error::Font);
            }
        }
        let mut metrics = Buffer::new(&self.metrics);
        metrics.reserve(r.chars.len());
        let mut present = [0; 4];
        for (index, i) in r.chars.iter().enumerate() {
            let character = r.bc + index;
            if i.width != 0 {
                present[character / 64] |= 1u64 << (character % 64);
            }
            metrics.push(Metrics {
                width: dimension(i.width)?,
                height: dimension(r.nw + i.height)?,
                depth: dimension(r.nw + r.nh + i.depth)?,
                italic: dimension(r.nw + r.nh + r.nd + i.italic)?,
            });
        }
        let mut kerns = Buffer::new(&self.dimensions);
        kerns.reserve(r.nk);
        for raw in r.bytes[r.kerns * 4..(r.kerns + r.nk) * 4]
            .as_chunks::<4>()
            .0
        {
            kerns.push(scale(*raw, size)?);
        }
        let mut params = Buffer::new(&self.dimensions);
        params.reserve(r.np.max(7));
        for i in 0..r.np.max(7) {
            let v = if i >= r.np {
                Scaled::ZERO
            } else {
                let raw = &r.bytes[(r.params + i) * 4..(r.params + i + 1) * 4];
                if i == 0 {
                    Scaled(i32::from_be_bytes(raw.try_into().unwrap()) >> 4)
                } else {
                    scale(raw.try_into().unwrap(), size)?
                }
            };
            params.push(v);
        }
        if self.scaled.is_empty() {
            self.scaled = Buffer::new(&self.scaled_store);
        }
        let id = self.scaled.len();
        self.sealed = true;
        if self
            .active
            .as_ref()
            .is_none_or(|c| c.len.get() == FONT_SLOTS)
        {
            self.active = Some(Rc::new(ScaledChunk {
                resources: self.resources.clone(),
                slots: std::array::from_fn(|_| OnceCell::new()),
                len: Cell::new(0),
            }));
        }
        let chunk = self.active.as_ref().unwrap();
        let slot = chunk.len.get();
        assert!(
            chunk.slots[slot]
                .set(ScaledFont {
                    resource,
                    size,
                    first: r.bc,
                    present,
                    metrics,
                    kerns,
                    params,
                })
                .is_ok()
        );
        chunk.len.set(slot + 1);
        self.scaled.push(FontResource {
            chunk: chunk.clone(),
            slot,
        });
        Ok(id)
    }
    pub fn metrics(&self, scaled: usize, c: u8) -> Option<Metrics> {
        self.scaled[scaled].descriptor().metrics(c)
    }
    pub fn metric_fields(&self, scaled: usize, c: u8) -> Option<Metrics> {
        self.scaled[scaled].descriptor().metric_fields(c)
    }
}
fn validate_fix(raw: &[u8]) -> Result<()> {
    if raw[0] != 0 && raw[0] != 255 {
        Err(Error::Font)
    } else {
        Ok(())
    }
}
pub(crate) fn ratio(value: i32, n: i32, d: i32) -> Result<i32> {
    let result = i64::from(value) * i64::from(n) / i64::from(d);
    if result.unsigned_abs() > 0x7fff_ffff {
        Err(Error::Overflow)
    } else {
        Ok(result as i32)
    }
}
fn scale(raw: [u8; 4], size: Scaled) -> Result<Scaled> {
    let [a, b, c, d] = raw;
    let mut z = i64::from(size.0);
    let mut alpha = 16;
    while z >= 8_388_608 {
        z /= 2;
        alpha *= 2;
    }
    let beta = 256 / alpha;
    alpha *= z;
    if beta == 0 {
        return Err(Error::Overflow);
    }
    let sw = (((i64::from(d) * z / 256 + i64::from(c) * z) / 256) + i64::from(b) * z) / beta;
    Ok(Scaled(if a == 0 { sw } else { sw - alpha } as i32))
}
#[cfg(test)]
#[path = "tfm_tests.rs"]
mod tests;
