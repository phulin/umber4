//! Logical loaded-font identities and exact resource bytes, without cache layout.
use super::*;
use crate::format::wire::{MAX_BYTES, Reader, Writer};

#[derive(Default)]
struct Budget {
    bytes: usize,
    search: usize,
}
impl Budget {
    fn resources(&mut self, count: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(
                count
                    .checked_mul(Catalog::format_resource_bytes())
                    .ok_or(Error::Capacity)?,
            )
            .ok_or(Error::Capacity)?;
        // Both the decoder and register_named check embedded-name uniqueness.
        self.search = self
            .search
            .checked_add(
                count
                    .checked_mul(count.saturating_sub(1))
                    .ok_or(Error::Capacity)?,
            )
            .ok_or(Error::Capacity)?;
        self.check()
    }
    fn add(&mut self, catalog: &Catalog, resource: usize, prior: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(catalog.scaled_allocation_bytes(resource))
            .ok_or(Error::Capacity)?;
        self.search = self.search.checked_add(prior).ok_or(Error::Capacity)?;
        self.check()
    }
    fn check(&self) -> Result<()> {
        if self.bytes > MAX_BYTES || self.search > MAX_BYTES {
            return Err(Error::Capacity);
        }
        Ok(())
    }
}

fn canonical(e: &Engine<'_>) -> bool {
    let fonts = &e.document.fonts;
    let f = fonts.descriptors[0];
    fonts.len == 1
        && fonts.current.value == 0
        && f.scaled.is_none()
        && f.count == 7
        && f.params == 0
        && fonts.parameter_end == 7
        && f.hyphen == 45
        && f.skew == -1
        && Some(f.identifier as u32) == e.names.lookup(b"nullfont")
        && (0..7).all(|i| fonts.parameters[i] == Scaled::ZERO)
}

pub(crate) fn write(e: &Engine<'_>, w: &mut Writer) -> Result<()> {
    let fonts = &e.document.fonts;
    if fonts.current.level != 0 {
        return Err(Error::Checkpoint);
    }
    if canonical(e) {
        w.u8(0);
        return Ok(());
    }
    w.u8(1);
    // A single file-ID directory, not a second representation of font payloads.
    let mut resources = Vec::new();
    let mut budget = Budget::default();
    for id in 1..fonts.len {
        let scaled = fonts.descriptors[id].scaled.unwrap();
        let resource = e.font_catalog.scaled[scaled].descriptor().resource;
        budget.add(&e.font_catalog, resource, id - 1)?;
        if !resources.iter().any(|&(r, _)| r == resource) {
            resources.push((resource, scaled));
        }
    }
    budget.resources(resources.len())?;
    w.count(resources.len())?;
    for &(_, scaled) in &resources {
        let handle = &e.font_catalog.scaled[scaled];
        w.blob(handle.name())?;
        w.blob(handle.tfm_bytes())?;
    }
    w.count(fonts.len)?;
    w.i32(fonts.current.value);
    for id in 0..fonts.len {
        let f = fonts.descriptors[id];
        let (resource, size) = if let Some(scaled) = f.scaled {
            let cached = e.font_catalog.scaled[scaled].descriptor();
            let ordinal = resources
                .iter()
                .position(|&(r, _)| r == cached.resource)
                .unwrap();
            (ordinal + 1, cached.size.0)
        } else {
            (0, 0)
        };
        w.count(resource)?;
        w.i32(size);
        w.count(f.count)?;
        w.i32(f.hyphen);
        w.i32(f.skew);
        w.u32(u32::try_from(f.identifier).map_err(|_| Error::Capacity)?);
        for i in 0..f.count {
            w.i32(fonts.parameters[f.params + i].0);
        }
    }
    Ok(())
}

pub(crate) fn read(
    e: &mut Engine<'_>,
    r: &mut Reader<'_>,
    name_count: usize,
    extras: &[(&[u8], &[u8])],
) -> Result<()> {
    match r.u8()? {
        0 => {
            for &(name, bytes) in extras {
                e.font_catalog.register_format_resource(name, bytes)?;
            }
            return Ok(());
        }
        1 => {}
        _ => return Err(Error::Format),
    }
    let n = r.count(8)?;
    let mut budget = Budget::default();
    budget.resources(n)?;
    // Resource IDs in this fresh catalog are the file ordinals. Reject repeated
    // embedded names; host overlays alone may repeat a byte-identical resource.
    for _ in 0..n {
        let name = r.blob()?;
        let bytes = r.blob()?;
        if name.is_empty()
            || e.font_catalog
                .resources
                .iter()
                .any(|resource| resource.name.as_ref() == name)
        {
            return Err(Error::Format);
        }
        e.font_catalog
            .register_format_embedded(name, bytes)
            .map_err(metric_error)?;
    }
    for &(name, bytes) in extras {
        e.font_catalog.register_format_resource(name, bytes)?;
    }
    // Every font has 24 fixed bytes and at least seven parameter words.
    let len = r.count(52)?;
    let current = r.i32()?;
    if len == 0 || current < 0 || current as usize >= len {
        return Err(Error::Format);
    }
    let mut fonts = Fonts::default();
    let mut used = vec![false; n];
    let mut end = 0usize;
    for id in 0..len {
        let resource = r.u32()? as usize;
        let size = r.i32()?;
        let count = r.count(4)?;
        let hyphen = r.i32()?;
        let skew = r.i32()?;
        let identifier = r.u32()? as usize;
        if count < 7 || identifier >= name_count {
            return Err(Error::Format);
        }
        // Take the entire parameter range before allocating scaled/cache data.
        let parameters = r.bytes(count.checked_mul(4).ok_or(Error::Format)?)?;
        let scaled = if id == 0 {
            if resource != 0 || size != 0 {
                return Err(Error::Format);
            }
            None
        } else {
            if resource == 0 || resource > n || size <= 0 {
                return Err(Error::Format);
            }
            used[resource - 1] = true;
            budget.add(&e.font_catalog, resource - 1, id - 1)?;
            let scaled = e
                .font_catalog
                .scaled(resource - 1, Scaled(size))
                .map_err(metric_error)?;
            if count < e.font_catalog.scaled[scaled].descriptor().params.len() {
                return Err(Error::Format);
            }
            Some(scaled)
        };
        fonts.descriptors[id] = Font {
            scaled,
            params: end,
            count,
            hyphen,
            skew,
            identifier,
        };
        for raw in parameters.as_chunks::<4>().0 {
            fonts.parameters[end] = Scaled(i32::from_le_bytes(*raw));
            end += 1;
        }
    }
    if used.contains(&false) {
        return Err(Error::Format);
    }
    fonts.len = len;
    fonts.parameter_end = end;
    fonts.current = Integer {
        value: current,
        level: 0,
    };
    e.document.fonts = fonts;
    Ok(())
}
fn metric_error(error: Error) -> Error {
    match error {
        Error::Font | Error::Overflow => Error::Format,
        other => other,
    }
}

#[cfg(test)]
#[path = "font_format_tests.rs"]
mod tests;
