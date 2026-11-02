//! File identities are temporary; owned nodes remain in their pooled store.
use super::*;
use crate::{
    Engine,
    format::wire::{Reader, Writer},
};
#[path = "node_format_fields.rs"]
mod fields;
#[path = "node_format_read.rs"]
mod reading;
#[path = "node_format_write.rs"]
mod writing;
pub(crate) use reading::read;
pub(crate) use writing::write;

fn checked(store: &NodeStore, id: NodeId) -> Result<&Node> {
    let offset = id.0.checked_sub(1).ok_or(Error::Format)? as usize;
    let node = store
        .pages
        .get(offset / CELLS)
        .and_then(Option::as_ref)
        .and_then(|p| p.data.as_ref())
        .map(|p| &p.nodes[offset % CELLS])
        .ok_or(Error::Format)?;
    if matches!(node.kind, Kind::Free | Kind::Unset(_)) {
        return Err(Error::Format);
    }
    Ok(node)
}
fn children(store: &NodeStore, kind: &Kind) -> [NodeId; 2] {
    let mut refs = [NodeId::NULL; 2];
    match kind {
        Kind::Box(id) => refs[0] = store.box_data(id).list.head(),
        Kind::Insertion(id) => refs[0] = store.insertion_data(id).list.head(),
        Kind::Leader(id) => refs[0] = store.leader_data(id).list.head(),
        Kind::Adjustment(list)
        | Kind::Ligature {
            components: list, ..
        } => refs[0] = list.head(),
        Kind::Discretionary { pre, post, .. } => refs = [pre.head(), post.head()],
        _ => {}
    }
    refs
}
fn discretionary_item(kind: &Kind) -> bool {
    matches!(
        kind,
        Kind::Character { .. }
            | Kind::Ligature { .. }
            | Kind::Kern { .. }
            | Kind::Box(_)
            | Kind::Rule { .. }
    )
}
/// The graph has already been checked for cycles/sharing before this list walk.
fn list_valid(store: &NodeStore, list: &List, accept: impl Fn(&Kind) -> bool) -> Result<()> {
    let mut at = list.head();
    let mut tail = NodeId::NULL;
    while !at.is_null() {
        let node = checked(store, at)?;
        if !accept(&node.kind) {
            return Err(Error::Format);
        }
        tail = at;
        at = node.next;
    }
    if tail != list.tail() {
        return Err(Error::Format);
    }
    Ok(())
}
fn ordinary_fields(e: &Engine<'_>, kind: &Kind) -> Result<()> {
    match kind {
        Kind::Character { font, .. } if *font as usize >= e.document.fonts.len => {
            return Err(Error::Format);
        }
        Kind::Ligature {
            font, character, ..
        } => {
            e.character_metrics(*font, *character)
                .ok_or(Error::Format)?;
        }
        Kind::Language(l) if !(1..=63).contains(&l.left) || !(1..=63).contains(&l.right) => {
            return Err(Error::Format);
        }
        Kind::Glue { subtype, .. } if subtype.is_leader() => return Err(Error::Format),
        _ => {}
    }
    Ok(())
}
fn ligature_components(store: &NodeStore, list: &List, font: u32) -> Result<()> {
    list_valid(
        store,
        list,
        |k| matches!(k, Kind::Character { font: f, .. } if *f == font),
    )
}
fn discretionary_lists(
    store: &NodeStore,
    pre: &List,
    post: &List,
    mut next: NodeId,
    count: u8,
) -> Result<()> {
    list_valid(store, pre, discretionary_item)?;
    list_valid(store, post, discretionary_item)?;
    for _ in 0..count {
        let replacement = checked(store, next)?;
        if !discretionary_item(&replacement.kind) {
            return Err(Error::Format);
        }
        next = replacement.next;
    }
    Ok(())
}
fn template(store: &NodeStore, list: &List, subtype: GlueKind) -> Result<()> {
    if list.is_empty()
        || list.head() != list.tail()
        || (!subtype.is_leader() && subtype != GlueKind::RightSkip)
    {
        return Err(Error::Format);
    }
    list_valid(store, list, |k| {
        matches!(k, Kind::Box(_) | Kind::Rule { .. })
    })
}
fn validate(e: &Engine<'_>, store: &NodeStore, id: NodeId) -> Result<()> {
    let node = checked(store, id)?;
    ordinary_fields(e, &node.kind)?;
    match &node.kind {
        Kind::Box(id) => {
            let data = store.box_data(id);
            if !data.glue_set.ratio.is_finite() {
                return Err(Error::Format);
            }
            list_valid(store, &data.list, |_| true)?;
        }
        Kind::Insertion(id) => {
            let data = store.insertion_data(id);
            if data.class == 255 {
                return Err(Error::Format);
            }
            list_valid(store, &data.list, |_| true)?;
        }
        Kind::Leader(id) => {
            let data = store.leader_data(id);
            template(store, &data.list, data.subtype)?;
        }
        Kind::Adjustment(list) => list_valid(store, list, |_| true)?,
        Kind::Ligature {
            components, font, ..
        } => {
            ligature_components(store, components, *font)?;
        }
        Kind::Discretionary {
            pre,
            post,
            replace_count,
        } => {
            discretionary_lists(store, pre, post, node.next, *replace_count)?;
        }
        _ => {}
    }
    Ok(())
}

/// Original ligature components can lack glyph metrics: the boundary program
/// runs before validating the original input character. Only their loaded font
/// identity and provenance structure apply; all rendered characters need metrics.
fn validate_rendered(
    e: &Engine<'_>,
    store: &NodeStore,
    roots: &[NodeId; 256],
    stack: &mut Vec<(NodeId, bool)>,
) -> Result<()> {
    debug_assert!(stack.is_empty());
    stack.extend(
        roots
            .iter()
            .filter(|id| !id.is_null())
            .map(|&id| (id, false)),
    );
    while let Some((mut id, component)) = stack.pop() {
        while !id.is_null() {
            let node = checked(store, id)?;
            if !component && let Kind::Character { font, character } = node.kind {
                e.character_metrics(font, character).ok_or(Error::Format)?;
            }
            for child in children(store, &node.kind) {
                if !child.is_null() {
                    stack.push((child, matches!(node.kind, Kind::Ligature { .. })));
                }
            }
            id = node.next;
        }
    }
    Ok(())
}
