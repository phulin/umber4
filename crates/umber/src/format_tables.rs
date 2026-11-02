use super::{
    validate,
    values::Values,
    wire::{Reader, Writer},
};
use crate::{
    Engine, Error, Result,
    layout_values::{DimenSlot, GlueSlot, LineShape, REGISTERS},
    numeric::{Glue, GlueOrder, Scaled},
    tables::Table,
    token::*,
    values::Owner,
};
pub(super) fn write_bindings(e: &Engine<'_>, values: &Values, w: &mut Writer) -> Result<()> {
    w.count(e.names.len())?;
    for i in 0..e.names.len() {
        let b = e.document.bindings[i];
        if b.level != 0 {
            return Err(Error::Checkpoint);
        }
        let mut m = b.meaning();
        if m.is_macro() {
            let owner = e.document.bindings.owner(i).ok_or(Error::Format)?;
            m.value = values.id(owner) as i32;
        }
        validate::meaning(m, values.owners.len(), e.document.fonts.len)?;
        w.u8(m.command);
        w.i32(m.value);
    }
    w.count(256 + cmd::names(cmd::TOKS_PARAM).len())?;
    for i in 0..256 + cmd::names(cmd::TOKS_PARAM).len() {
        let v = &e.document.token_registers[i];
        if v.level != 0 {
            return Err(Error::Checkpoint);
        }
        w.u32(v.owner.as_ref().map_or(u32::MAX, |o| values.id(o)));
    }
    w.u32(e.document.par.0);
    Ok(())
}
pub(super) fn read_bindings(
    e: &mut Engine<'_>,
    values: &[Owner],
    r: &mut Reader<'_>,
) -> Result<()> {
    if r.count(5)? != e.names.len() {
        return Err(Error::Format);
    }
    for i in 0..e.names.len() {
        let mut m = Meaning {
            command: r.u8()?,
            value: r.i32()?,
        };
        validate::meaning(m, values.len(), e.document.fonts.len)?;
        if [0, END_WRITE_ID, END_TEMPLATE_ID, ENDV_ID].contains(&(i as u32))
            && m != e.document.bindings[i].meaning()
        {
            return Err(Error::Format);
        }
        let owner = if m.is_macro() {
            let owner = &values[m.value as usize];
            if owner.definition().body.start == 0 {
                return Err(Error::Format);
            }
            m.value = owner.id as i32;
            Some(owner.clone())
        } else {
            None
        };
        e.document.bindings[i] = Binding::new(m, 0);
        e.document.bindings.replace_owner(i, owner);
    }
    let count = 256 + cmd::names(cmd::TOKS_PARAM).len();
    if r.count(4)? != count {
        return Err(Error::Format);
    }
    for i in 0..count {
        let value = r.u32()?;
        let owner = if value == u32::MAX {
            None
        } else {
            Some(values.get(value as usize).ok_or(Error::Format)?.clone())
        };
        e.document.token_registers[i] = TokenValue { owner, level: 0 };
    }
    if r.u32()? != e.document.par.0 {
        return Err(Error::Format);
    }
    Ok(())
}
fn write_glue(w: &mut Writer, g: Glue) {
    w.i32(g.width.0);
    w.i32(g.stretch.0);
    w.i32(g.shrink.0);
    w.u8(g.stretch_order as u8);
    w.u8(g.shrink_order as u8);
}
fn order(r: &mut Reader<'_>) -> Result<GlueOrder> {
    match r.u8()? {
        0 => Ok(GlueOrder::Normal),
        1 => Ok(GlueOrder::Fil),
        2 => Ok(GlueOrder::Fill),
        3 => Ok(GlueOrder::Filll),
        _ => Err(Error::Format),
    }
}
fn read_glue(r: &mut Reader<'_>) -> Result<Glue> {
    Ok(Glue {
        width: Scaled(r.i32()?),
        stretch: Scaled(r.i32()?),
        shrink: Scaled(r.i32()?),
        stretch_order: order(r)?,
        shrink_order: order(r)?,
    })
}
pub(super) fn write_layout(e: &Engine<'_>, w: &mut Writer) -> Result<()> {
    let l = &e.document.layout_values;
    w.count(l.dimens.len())?;
    for i in 0..l.dimens.len() {
        let s = l.dimens[i];
        if s.level != 0 {
            return Err(Error::Checkpoint);
        }
        w.i32(s.value.0);
    }
    for table in [&l.glues, &l.mu_glues] {
        w.count(table.len())?;
        for i in 0..table.len() {
            let s = table[i];
            if s.level != 0 {
                return Err(Error::Checkpoint);
            }
            write_glue(w, s.value);
        }
    }
    if l.shape.level != 0 {
        return Err(Error::Checkpoint);
    }
    let count = l.shape.lines.as_ref().map_or(0, |t| t.len());
    w.count(count)?;
    if let Some(shape) = &l.shape.lines {
        for i in 0..count {
            w.i32(shape[i].indent.0);
            w.i32(shape[i].width.0);
        }
    }
    Ok(())
}
fn count(r: &mut Reader<'_>, minimum: usize, maximum: usize) -> Result<usize> {
    let n = r.count(minimum)?;
    if !(REGISTERS..=maximum).contains(&n) {
        return Err(Error::Format);
    }
    Ok(n)
}
pub(super) fn read_layout(e: &mut Engine<'_>, r: &mut Reader<'_>) -> Result<()> {
    let n = count(r, 4, REGISTERS + cmd::names(cmd::DIMEN_PARAM).len())?;
    for i in 0..n {
        e.document.layout_values.dimens[i] = DimenSlot {
            value: Scaled(r.i32()?),
            level: 0,
        };
    }
    for (table, max) in [
        (
            &mut e.document.layout_values.glues,
            REGISTERS + cmd::names(cmd::GLUE_PARAM).len(),
        ),
        (
            &mut e.document.layout_values.mu_glues,
            REGISTERS + cmd::names(cmd::MU_GLUE_PARAM).len(),
        ),
    ] {
        let n = count(r, 14, max)?;
        for i in 0..n {
            table[i] = GlueSlot {
                value: read_glue(r)?,
                level: 0,
            };
        }
    }
    let n = r.count(8)?;
    if n != 0 {
        let mut shape = Table::new(n);
        for i in 0..n {
            shape[i] = LineShape {
                indent: Scaled(r.i32()?),
                width: Scaled(r.i32()?),
            };
        }
        e.document.layout_values.shape.lines = Some(shape);
    }
    Ok(())
}
