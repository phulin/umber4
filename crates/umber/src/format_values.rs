//! Reachable immutable Values decode into the normal pooled headers/chunks.
use super::{
    validate,
    wire::{Reader, Writer},
};
use crate::{
    Engine, Error, Result,
    token::*,
    values::{Builder, Class, Owner},
};
use std::collections::HashMap;
pub(super) struct Values {
    pub owners: Vec<Owner>,
    ids: HashMap<u32, u32>,
}
impl Values {
    pub fn collect(e: &Engine<'_>) -> Self {
        let mut out = Self {
            owners: Vec::new(),
            ids: HashMap::new(),
        };
        for id in 0..e.names.len() {
            if let Some(owner) = e.document.bindings.owner(id) {
                out.add(owner);
            }
        }
        for i in 0..256 + cmd::names(cmd::TOKS_PARAM).len() {
            if let Some(owner) = &e.document.token_registers[i].owner {
                out.add(owner);
            }
        }
        out
    }
    fn add(&mut self, owner: &Owner) {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.ids.entry(owner.id) {
            entry.insert(self.owners.len() as u32);
            self.owners.push(owner.clone());
        }
    }
    pub fn id(&self, owner: &Owner) -> u32 {
        self.ids[&owner.id]
    }
    pub fn write(&self, e: &Engine<'_>, w: &mut Writer) -> Result<()> {
        w.count(self.owners.len())?;
        for owner in &self.owners {
            let d = owner.definition();
            let is_macro = d.body.start != 0;
            w.u16(u16::from(is_macro) | (u16::from(d.long) << 1));
            w.u8(d.arity as u8);
            w.u32(d.prefix_end);
            w.u32(d.body.start);
            w.count(owner.tokens.len as usize)?;
            for i in 0..owner.tokens.len {
                let t = owner.tokens.get(i);
                validate::token(t, e.names.len(), e.document.fonts.len, is_macro)?;
                w.u32(t.0);
            }
        }
        Ok(())
    }
}
pub(super) fn read(e: &mut Engine<'_>, r: &mut Reader<'_>) -> Result<Vec<Owner>> {
    let count = r.count(15)?;
    let mut owners = Vec::with_capacity(count);
    for _ in 0..count {
        let flags = r.u16()?;
        let arity = r.u8()?;
        let prefix = r.u32()?;
        let body = r.u32()?;
        let length = r.count(4)?;
        if flags > 3 || arity > 9 {
            return Err(Error::Format);
        }
        let is_macro = flags & 1 != 0;
        let mut b = e.values.builder(Class::Durable)?;
        for _ in 0..length {
            let t = Token(r.u32()?);
            validate::token(t, e.names.len(), e.document.fonts.len, is_macro)?;
            b.push(t)?;
        }
        if is_macro {
            definition(&mut b, prefix, body, arity, flags & 2 != 0)?;
        } else if flags != 0 || arity != 0 || prefix != 0 || body != 0 {
            return Err(Error::Format);
        }
        owners.push(b.finish());
    }
    Ok(owners)
}
fn definition(b: &mut Builder, prefix: u32, body: u32, arity: u8, long: bool) -> Result<()> {
    if body == 0 || body > b.len() || prefix >= body || b.get(body - 1) != Token(PARAM | 255) {
        return Err(Error::Format);
    }
    for i in 0..prefix {
        if b.get(i).is_parameter() {
            return Err(Error::Format);
        }
    }
    let mut at = prefix;
    for _ in 0..arity {
        if at >= body - 1 || !b.get(at).is_parameter() {
            return Err(Error::Format);
        }
        let start = at + 1;
        at = start;
        while at < body - 1 && !b.get(at).is_parameter() {
            at += 1;
        }
        let len = (at - start) as usize;
        let failure_start = b.definition_mut().failure.len();
        b.definition_mut().failure.resize(failure_start + len, 0);
        let mut matched = 0;
        for i in 1..len {
            let t = b.get(start + i as u32);
            while matched > 0 && b.get(start + matched as u32) != t {
                matched = b.definition_mut().failure[failure_start + matched - 1];
            }
            if b.get(start + matched as u32) == t {
                matched += 1;
            }
            b.definition_mut().failure[failure_start + i] = matched;
        }
        b.definition_mut().parameters.push(Parameter {
            delimiter: ArenaRange { start, end: at },
            failure_start,
        });
    }
    if at != body - 1 {
        return Err(Error::Format);
    }
    let mut uses_parameters = false;
    for i in body..b.len() {
        let token = b.get(i);
        if token.is_parameter() {
            if !(1..=u32::from(arity)).contains(&(token.0 & 255)) {
                return Err(Error::Format);
            }
            uses_parameters = true;
        }
    }
    let len = b.len();
    let d = b.definition_mut();
    d.prefix_end = prefix;
    d.body = ArenaRange {
        start: body,
        end: len,
    };
    d.arity = Arity::new(arity);
    d.long = long;
    d.uses_parameters = uses_parameters;
    Ok(())
}
