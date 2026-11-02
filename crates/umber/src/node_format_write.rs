//! Indexed postorder over borrowed owners; no live record is copied or moved.
use super::*;
pub(crate) fn write(e: &Engine<'_>, w: &mut Writer, roots: &[NodeId; 256]) -> Result<()> {
    let count = roots.iter().filter(|r| !r.is_null()).count();
    if count == 0 {
        w.u8(0);
        return Ok(());
    }
    let store = &e.document.nodes;
    let addresses = store
        .pages
        .len()
        .checked_mul(CELLS)
        .ok_or(Error::Capacity)?;
    let live: usize = store
        .pages
        .iter()
        .flatten()
        .map(|p| usize::from(p.data.as_ref().unwrap().live))
        .sum();
    let mut ids = Vec::new();
    ids.try_reserve_exact(addresses)
        .map_err(|_| Error::Capacity)?;
    ids.resize(addresses, 0u32);
    let mut order = Vec::new();
    order.try_reserve_exact(live).map_err(|_| Error::Capacity)?;
    let mut stack = Vec::new();
    stack
        .try_reserve_exact(live.checked_mul(2).ok_or(Error::Capacity)?)
        .map_err(|_| Error::Capacity)?;
    for &root in roots {
        if root.is_null() {
            continue;
        }
        let node = checked(store, root)?;
        if !node.next.is_null() || !matches!(node.kind, Kind::Box(_)) {
            return Err(Error::Format);
        }
        stack.push((root, false));
        while let Some((id, finish)) = stack.pop() {
            let node = checked(store, id)?;
            let index = id.0 as usize - 1;
            if finish {
                let next = u32::try_from(order.len() + 1).map_err(|_| Error::Capacity)?;
                if next == u32::MAX {
                    return Err(Error::Capacity);
                }
                ids[index] = next;
                order.push(id);
            } else {
                if ids[index] != 0 {
                    return Err(Error::Format);
                }
                ids[index] = u32::MAX;
                stack.push((id, true));
                if !node.next.is_null() {
                    stack.push((node.next, false));
                }
                for child in children(store, &node.kind).into_iter().rev() {
                    if !child.is_null() {
                        stack.push((child, false));
                    }
                }
            }
        }
    }
    for &id in &order {
        validate(e, store, id)?;
    }
    validate_rendered(e, store, roots, &mut stack)?;
    w.u8(1);
    w.count(count)?;
    w.count(order.len())?;
    let reference = |id: NodeId| {
        if id.is_null() {
            0
        } else {
            ids[id.0 as usize - 1]
        }
    };
    for (i, &root) in roots.iter().enumerate() {
        if !root.is_null() {
            w.u8(i as u8);
            w.u32(reference(root));
        }
    }
    for id in order {
        let node = store.get(id);
        w.u8(match node.kind {
            Kind::Language(_) => 0,
            Kind::Character { .. } => 1,
            Kind::Ligature { .. } => 2,
            Kind::Kern { .. } => 3,
            Kind::MathBoundary { .. } => 4,
            Kind::Glue { .. } => 5,
            Kind::Penalty(_) => 6,
            Kind::Rule { .. } => 7,
            Kind::Box(_) => 8,
            Kind::Insertion(_) => 9,
            Kind::Leader(_) => 10,
            Kind::Adjustment(_) => 11,
            Kind::Discretionary { .. } => 12,
            Kind::Free | Kind::Unset(_) => return Err(Error::Format),
        });
        w.u32(reference(node.next));
        match &node.kind {
            Kind::Language(l) => {
                w.u8(l.number);
                w.u8(l.left);
                w.u8(l.right);
            }
            Kind::Character { font, character } => {
                w.u32(*font);
                w.u8(*character);
            }
            Kind::Ligature {
                font,
                character,
                components,
                left_boundary,
                right_boundary,
            } => {
                w.u32(*font);
                w.u8(*character);
                w.u8(u8::from(*left_boundary));
                w.u8(u8::from(*right_boundary));
                w.u32(reference(components.head()));
            }
            Kind::Kern { width, kind } => {
                w.i32(width.0);
                fields::write_kern(w, *kind);
            }
            Kind::MathBoundary { width, after } => {
                w.i32(width.0);
                w.u8(u8::from(*after));
            }
            Kind::Glue { spec, subtype } => {
                fields::write_glue(w, *spec);
                fields::write_subtype(w, *subtype);
            }
            Kind::Penalty(p) => w.i32(*p),
            Kind::Rule {
                width,
                height,
                depth,
            } => {
                w.i32(width.0);
                w.i32(height.0);
                w.i32(depth.0);
            }
            Kind::Box(id) => {
                let d = store.box_data(id);
                w.u8(u8::from(d.vertical));
                for value in [d.width, d.height, d.depth, d.shift] {
                    w.i32(value.0);
                }
                fields::write_setting(w, d.glue_set);
                w.u32(reference(d.list.head()));
            }
            Kind::Insertion(id) => {
                let d = store.insertion_data(id);
                w.u8(d.class);
                w.i32(d.height.0);
                fields::write_glue(w, d.split_top);
                w.i32(d.max_depth.0);
                w.i32(d.float_cost);
                w.u32(reference(d.list.head()));
            }
            Kind::Leader(id) => {
                let d = store.leader_data(id);
                fields::write_glue(w, d.spec);
                fields::write_subtype(w, d.subtype);
                w.u32(reference(d.list.head()));
            }
            Kind::Adjustment(list) => w.u32(reference(list.head())),
            Kind::Discretionary {
                pre,
                post,
                replace_count,
            } => {
                w.u32(reference(pre.head()));
                w.u32(reference(post.head()));
                w.u8(*replace_count);
            }
            Kind::Free | Kind::Unset(_) => return Err(Error::Format),
        }
    }
    Ok(())
}
