//! Consume each file reference once while constructing final pooled nodes.
use super::*;
enum Body {
    Ordinary(Kind),
    Box(BoxData),
    Insertion(InsertionData),
    Leader(LeaderData),
}
struct Record {
    body: Body,
    next: u32,
    children: [u32; 2],
}
impl Record {
    fn read(r: &mut Reader<'_>) -> Result<Self> {
        let tag = r.u8()?;
        let next = r.u32()?;
        let mut children = [0; 2];
        let body = match tag {
            0 => Body::Ordinary(Kind::Language(crate::language::Language {
                number: r.u8()?,
                left: r.u8()?,
                right: r.u8()?,
            })),
            1 => Body::Ordinary(Kind::Character {
                font: r.u32()?,
                character: r.u8()?,
            }),
            2 => {
                let (font, character) = (r.u32()?, r.u8()?);
                let (left, right) = (fields::boolean(r)?, fields::boolean(r)?);
                children[0] = r.u32()?;
                Body::Ordinary(Kind::ligature(
                    font,
                    character,
                    List::default(),
                    left,
                    right,
                ))
            }
            3 => Body::Ordinary(Kind::Kern {
                width: Scaled(r.i32()?),
                kind: fields::kern(r)?,
            }),
            4 => Body::Ordinary(Kind::MathBoundary {
                width: Scaled(r.i32()?),
                after: fields::boolean(r)?,
            }),
            5 => Body::Ordinary(Kind::Glue {
                spec: fields::glue(r)?,
                subtype: fields::subtype(r)?,
            }),
            6 => Body::Ordinary(Kind::Penalty(r.i32()?)),
            7 => Body::Ordinary(Kind::Rule {
                width: Scaled(r.i32()?),
                height: Scaled(r.i32()?),
                depth: Scaled(r.i32()?),
            }),
            8 => {
                let data = BoxData {
                    vertical: fields::boolean(r)?,
                    width: Scaled(r.i32()?),
                    height: Scaled(r.i32()?),
                    depth: Scaled(r.i32()?),
                    shift: Scaled(r.i32()?),
                    glue_set: fields::setting(r)?,
                    ..BoxData::default()
                };
                children[0] = r.u32()?;
                Body::Box(data)
            }
            9 => {
                let data = InsertionData {
                    class: r.u8()?,
                    height: Scaled(r.i32()?),
                    split_top: fields::glue(r)?,
                    max_depth: Scaled(r.i32()?),
                    float_cost: r.i32()?,
                    ..InsertionData::default()
                };
                children[0] = r.u32()?;
                Body::Insertion(data)
            }
            10 => {
                let data = LeaderData {
                    spec: fields::glue(r)?,
                    subtype: fields::subtype(r)?,
                    ..LeaderData::default()
                };
                children[0] = r.u32()?;
                Body::Leader(data)
            }
            11 => {
                children[0] = r.u32()?;
                Body::Ordinary(Kind::Adjustment(List::default()))
            }
            12 => {
                children = [r.u32()?, r.u32()?];
                Body::Ordinary(Kind::discretionary(
                    List::default(),
                    List::default(),
                    r.u8()?,
                ))
            }
            _ => return Err(Error::Format),
        };
        Ok(Self {
            body,
            next,
            children,
        })
    }
    fn check(&self, e: &Engine<'_>, store: &NodeStore, map: &[Option<List>]) -> Result<()> {
        let empty = List::default();
        let lists = self.children.map(|id| {
            if id == 0 {
                &empty
            } else {
                map[id as usize].as_ref().unwrap()
            }
        });
        match &self.body {
            Body::Ordinary(kind) => {
                ordinary_fields(e, kind)?;
                match kind {
                    Kind::Ligature { font, .. } => ligature_components(store, lists[0], *font)?,
                    Kind::Discretionary { replace_count, .. } => {
                        let next = if self.next == 0 {
                            NodeId::NULL
                        } else {
                            map[self.next as usize].as_ref().unwrap().head()
                        };
                        discretionary_lists(store, lists[0], lists[1], next, *replace_count)?;
                    }
                    _ => {}
                }
            }
            Body::Insertion(d) if d.class == 255 => return Err(Error::Format),
            Body::Leader(d) => template(store, lists[0], d.subtype)?,
            _ => {}
        }
        Ok(())
    }
    fn allocate(self, store: &mut NodeStore, mut children: [List; 2]) -> Result<DetachedNode> {
        match self.body {
            Body::Box(mut d) => {
                d.list = std::mem::take(&mut children[0]);
                store.alloc_box(d)
            }
            Body::Insertion(mut d) => {
                d.list = std::mem::take(&mut children[0]);
                store.alloc_insertion(d)
            }
            Body::Leader(mut d) => {
                d.list = std::mem::take(&mut children[0]);
                store.alloc_leader(d)
            }
            Body::Ordinary(mut kind) => {
                match &mut kind {
                    Kind::Adjustment(list)
                    | Kind::Ligature {
                        components: list, ..
                    } => *list = std::mem::take(&mut children[0]),
                    Kind::Discretionary { pre, post, .. } => {
                        *pre = std::mem::take(&mut children[0]);
                        *post = std::mem::take(&mut children[1]);
                    }
                    _ => {}
                }
                store.alloc(kind)
            }
        }
    }
}
fn take(map: &mut [Option<List>], id: u32) -> List {
    if id == 0 {
        List::default()
    } else {
        map[id as usize].take().expect("validated affine reference")
    }
}
pub(crate) fn read(
    e: &Engine<'_>,
    r: &mut Reader<'_>,
    store: &mut NodeStore,
) -> Result<[List; 256]> {
    match r.u8()? {
        0 => {
            r.finish()?;
            return Ok(std::array::from_fn(|_| List::default()));
        }
        1 => {}
        _ => return Err(Error::Format),
    }
    let root_count = r.count(5)?;
    if !(1..=256).contains(&root_count) {
        return Err(Error::Format);
    }
    let count = r.count(8)?;
    if count < root_count {
        return Err(Error::Format);
    }
    let mut roots = [0u32; 256];
    for _ in 0..root_count {
        let register = r.u8()? as usize;
        let id = r.u32()?;
        if id == 0 || id as usize > count || roots[register] != 0 {
            return Err(Error::Format);
        }
        roots[register] = id;
    }
    let mut sorted = roots;
    sorted.sort_unstable();
    if sorted.windows(2).any(|p| p[0] != 0 && p[0] == p[1]) {
        return Err(Error::Format);
    }
    let mut map = Vec::new();
    map.try_reserve_exact(count + 1)
        .map_err(|_| Error::Capacity)?;
    map.resize_with(count + 1, || None);
    let result = (|| {
        for current in 1..=count {
            let record = Record::read(r)?;
            let refs = [record.next, record.children[0], record.children[1]];
            for (i, &id) in refs.iter().enumerate() {
                if id != 0
                    && (id as usize >= current
                        || map[id as usize].is_none()
                        || refs[..i].contains(&id))
                {
                    return Err(Error::Format);
                }
            }
            record.check(e, store, &map)?;
            // Parsing and ownership checks finish before taking any children.
            // Allocation consumes/releases them on capacity failure; the next
            // chain stays mapped until the new parent has an established owner.
            let children = record.children.map(|id| take(&mut map, id));
            let next = record.next;
            let node = record.allocate(store, children)?;
            let mut list = node.into_list();
            store.splice(&mut list, take(&mut map, next));
            map[current] = Some(list);
        }
        r.finish()?;
        if map.iter().filter(|v| v.is_some()).count() != root_count {
            return Err(Error::Format);
        }
        for &id in roots.iter().filter(|&&id| id != 0) {
            let list = map[id as usize].as_ref().ok_or(Error::Format)?;
            if list.head() != list.tail() || !matches!(store.get(list.head()).kind, Kind::Box(_)) {
                return Err(Error::Format);
            }
        }
        let runtime = roots.map(|id| {
            if id == 0 {
                NodeId::NULL
            } else {
                map[id as usize].as_ref().unwrap().head()
            }
        });
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(count)
            .map_err(|_| Error::Capacity)?;
        validate_rendered(e, store, &runtime, &mut stack)?;
        Ok(roots.map(|id| take(&mut map, id)))
    })();
    if result.is_err() {
        let mut release = ReleaseScratch::default();
        for list in map.into_iter().flatten() {
            store.release(list, &mut release);
        }
    }
    result
}
