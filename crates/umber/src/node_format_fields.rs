//! Checked scalar tags, shared by the ordinary node reader and writer.
use super::*;
const SUBTYPES: [GlueKind; 15] = [
    GlueKind::Normal,
    GlueKind::LeftSkip,
    GlueKind::RightSkip,
    GlueKind::ParFillSkip,
    GlueKind::BaselineSkip,
    GlueKind::LineSkip,
    GlueKind::ParSkip,
    GlueKind::TopSkip,
    GlueKind::SpaceSkip,
    GlueKind::XSpaceSkip,
    GlueKind::AlignedLeaders,
    GlueKind::CenteredLeaders,
    GlueKind::ExpandedLeaders,
    GlueKind::TabSkip,
    GlueKind::NonScript,
];
pub(super) fn boolean(r: &mut Reader<'_>) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::Format),
    }
}
pub(super) fn order(r: &mut Reader<'_>) -> Result<GlueOrder> {
    match r.u8()? {
        0 => Ok(GlueOrder::Normal),
        1 => Ok(GlueOrder::Fil),
        2 => Ok(GlueOrder::Fill),
        3 => Ok(GlueOrder::Filll),
        _ => Err(Error::Format),
    }
}
pub(super) fn subtype(r: &mut Reader<'_>) -> Result<GlueKind> {
    SUBTYPES.get(r.u8()? as usize).copied().ok_or(Error::Format)
}
pub(super) fn write_subtype(w: &mut Writer, kind: GlueKind) {
    w.u8(match kind {
        GlueKind::Normal => 0,
        GlueKind::LeftSkip => 1,
        GlueKind::RightSkip => 2,
        GlueKind::ParFillSkip => 3,
        GlueKind::BaselineSkip => 4,
        GlueKind::LineSkip => 5,
        GlueKind::ParSkip => 6,
        GlueKind::TopSkip => 7,
        GlueKind::SpaceSkip => 8,
        GlueKind::XSpaceSkip => 9,
        GlueKind::AlignedLeaders => 10,
        GlueKind::CenteredLeaders => 11,
        GlueKind::ExpandedLeaders => 12,
        GlueKind::TabSkip => 13,
        GlueKind::NonScript => 14,
    });
}
pub(super) fn glue(r: &mut Reader<'_>) -> Result<Glue> {
    Ok(Glue {
        width: Scaled(r.i32()?),
        stretch: Scaled(r.i32()?),
        shrink: Scaled(r.i32()?),
        stretch_order: order(r)?,
        shrink_order: order(r)?,
    })
}
pub(super) fn write_glue(w: &mut Writer, spec: Glue) {
    w.i32(spec.width.0);
    w.i32(spec.stretch.0);
    w.i32(spec.shrink.0);
    w.u8(spec.stretch_order as u8);
    w.u8(spec.shrink_order as u8);
}
pub(super) fn kern(r: &mut Reader<'_>) -> Result<KernKind> {
    match r.u8()? {
        0 => Ok(KernKind::Normal),
        1 => Ok(KernKind::Explicit),
        2 => Ok(KernKind::Accent),
        _ => Err(Error::Format),
    }
}
pub(super) fn write_kern(w: &mut Writer, kind: KernKind) {
    w.u8(match kind {
        KernKind::Normal => 0,
        KernKind::Explicit => 1,
        KernKind::Accent => 2,
    });
}
pub(super) fn setting(r: &mut Reader<'_>) -> Result<GlueSet> {
    let sign = match r.u8()? {
        0 => GlueSign::Normal,
        1 => GlueSign::Stretching,
        2 => GlueSign::Shrinking,
        _ => return Err(Error::Format),
    };
    let order = order(r)?;
    let ratio = f64::from_bits(r.u64()?);
    if !ratio.is_finite() {
        return Err(Error::Format);
    }
    Ok(GlueSet { sign, order, ratio })
}
pub(super) fn write_setting(w: &mut Writer, setting: GlueSet) {
    w.u8(match setting.sign {
        GlueSign::Normal => 0,
        GlueSign::Stretching => 1,
        GlueSign::Shrinking => 2,
    });
    w.u8(setting.order as u8);
    w.u64(setting.ratio.to_bits());
}
