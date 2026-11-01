//! Visited-glue word eligibility and synchronized discretionary reconstruction.
use crate::nodes::KernKind;
use crate::{
    Result,
    fonts::Fonts,
    hyphenation::Hyphenation,
    language::Language,
    nodes::{Kind, List, NodeId, NodeStore, ReleaseScratch},
    scalars::{Scalars, Target},
    tfm::Catalog,
};
#[path = "hyphenation_reconstitute.rs"]
mod reconstitution;
use reconstitution::{Initial, Reconstituter};

pub(crate) struct Inputs<'a> {
    pub scalars: &'a Scalars,
    pub fonts: &'a Fonts,
    pub catalog: &'a Catalog,
    pub tables: &'a Hyphenation,
}
#[derive(Default)]
pub(crate) struct Scratch {
    reconstitution: reconstitution::Scratch,
    release: ReleaseScratch,
}
struct Word {
    original: [u16; 64],
    lower: [u8; 63],
    length: usize,
    before: NodeId,
    before_before: NodeId,
    last: NodeId,
    font: u32,
    hyphen: u16,
    boundary: u16,
}

pub(crate) fn after_glue(
    glue: NodeId,
    nodes: &mut NodeStore,
    paragraph: &mut List,
    inputs: &Inputs<'_>,
    language: &mut Language,
    scratch: &mut Scratch,
) -> Result<()> {
    let Some(mut word) = candidate(nodes, glue, inputs, language) else {
        return Ok(());
    };
    let positions = inputs.tables.positions(
        language.number,
        &word.lower[..word.length],
        language.left.into(),
        language.right.into(),
    );
    if positions == 0 {
        return Ok(());
    }
    let (previous, j, initial) = initial(nodes, &mut word);
    let resource =
        &inputs.catalog.scaled[inputs.fonts.descriptors[word.font as usize].scaled.unwrap()];
    let mut rebuilt = List::default();
    let mut branches = Branches::default();
    let result = rebuild(
        &mut Reconstituter {
            nodes,
            font: word.font,
            metrics: resource.descriptor(),
            resource: resource.source(),
            original: &mut word.original,
            initial,
            scratch: &mut scratch.reconstitution,
        },
        j,
        word.length,
        word.boundary,
        word.hyphen,
        positions,
        &mut rebuilt,
        &mut branches,
        &mut scratch.release,
    );
    branches.release(nodes, &mut scratch.release);
    if let Err(error) = result {
        nodes.release(rebuilt, &mut scratch.release);
        return Err(error);
    }
    // Original and rebuilt trees have remained disjoint. All allocation is
    // complete before transferring the new range into the paragraph root.
    let suffix = nodes.split_after(paragraph, word.last);
    let original = nodes.split_after(paragraph, previous);
    nodes.splice(paragraph, rebuilt);
    nodes.splice(paragraph, suffix);
    nodes.release(original, &mut scratch.release);
    Ok(())
}

fn candidate(
    nodes: &NodeStore,
    glue: NodeId,
    inputs: &Inputs<'_>,
    language: &mut Language,
) -> Option<Word> {
    let mut before_before = NodeId::NULL;
    let mut before = glue;
    let mut s = nodes.get(glue).next;
    let font = loop {
        if s.is_null() {
            return None;
        }
        let node = nodes.get(s);
        let letter = match &node.kind {
            Kind::Character { font, character } => Some((*font, *character)),
            Kind::Ligature { components, .. } if !components.is_empty() => {
                let Kind::Character { font, character } = nodes.get(components.head()).kind else {
                    unreachable!()
                };
                Some((font, character))
            }
            Kind::Ligature { .. }
            | Kind::Kern {
                kind: KernKind::Normal,
                ..
            } => None,
            Kind::Language(new) => {
                *language = *new;
                None
            }
            _ => return None,
        };
        if let Some((font, character)) = letter {
            let lower = inputs.scalars[Target::casecode(0, character as usize)].value;
            if lower != 0 {
                if lower != character.into() && inputs.scalars[Target::UC_HYPH].value <= 0 {
                    return None;
                }
                break font;
            }
        }
        before_before = before;
        before = s;
        s = node.next;
    };
    let descriptor = inputs.fonts.descriptors[font as usize];
    if !(0..=255).contains(&descriptor.hyphen)
        || u16::from(language.left) + u16::from(language.right) > 63
    {
        return None;
    }
    let font_boundary = inputs.catalog.scaled[descriptor.scaled?].source().boundary;
    let mut word = Word {
        original: [0; 64],
        lower: [0; 63],
        length: 0,
        before,
        before_before,
        last: NodeId::NULL,
        font,
        hyphen: descriptor.hyphen as u16,
        boundary: 256,
    };
    'letters: loop {
        if s.is_null() {
            return None;
        }
        let node = nodes.get(s);
        match &node.kind {
            Kind::Character { font: f, character } => {
                if *f != font {
                    break;
                }
                word.boundary = (*character).into();
                let lower = inputs.scalars[Target::casecode(0, *character as usize)].value;
                if lower == 0 || word.length == 63 {
                    break;
                }
                word.original[word.length + 1] = (*character).into();
                word.lower[word.length] = lower as u8;
                word.length += 1;
                word.last = s;
                word.boundary = 256;
            }
            Kind::Ligature {
                font: f,
                components,
                right_boundary,
                ..
            } => {
                if *f != font {
                    break;
                }
                let mut j = word.length;
                let mut p = components.head();
                if !p.is_null() {
                    let Kind::Character { character, .. } = nodes.get(p).kind else {
                        unreachable!()
                    };
                    word.boundary = character.into();
                }
                while !p.is_null() {
                    let component = nodes.get(p);
                    let Kind::Character { character, .. } = component.kind else {
                        unreachable!()
                    };
                    let lower = inputs.scalars[Target::casecode(0, character as usize)].value;
                    if lower == 0 || j == 63 {
                        break 'letters;
                    }
                    word.original[j + 1] = character.into();
                    word.lower[j] = lower as u8;
                    j += 1;
                    p = component.next;
                }
                word.length = j;
                word.last = s;
                word.boundary = if *right_boundary { font_boundary } else { 256 };
            }
            Kind::Kern {
                kind: KernKind::Normal,
                ..
            } => {
                word.last = s;
                word.boundary = font_boundary;
            }
            _ => break,
        }
        s = node.next;
    }
    if word.length < usize::from(language.left) + usize::from(language.right) {
        return None;
    }
    // Characters beyond the candidate (including a different font or a prefix
    // exceeding 63 letters) still participate in the terminating-node check.
    loop {
        if s.is_null() {
            return None;
        }
        let node = nodes.get(s);
        match node.kind {
            Kind::Character { .. }
            | Kind::Ligature { .. }
            | Kind::Kern {
                kind: KernKind::Normal,
                ..
            } => {}
            Kind::Language(_)
            | Kind::Glue { .. }
            | Kind::Leader(_)
            | Kind::Penalty(_)
            | Kind::Insertion(_)
            | Kind::Adjustment(_)
            | Kind::Kern {
                kind: KernKind::Explicit | KernKind::Accent,
                ..
            } => return Some(word),
            _ => return None,
        }
        s = node.next;
    }
}

fn initial(nodes: &NodeStore, word: &mut Word) -> (NodeId, usize, Initial) {
    let node = nodes.get(word.before);
    match &node.kind {
        Kind::Character { font, character } if *font == word.font => {
            word.original[0] = (*character).into();
            (
                word.before_before,
                0,
                Initial {
                    components: word.before,
                    ..Initial::default()
                },
            )
        }
        Kind::Ligature {
            font,
            character,
            components,
            left_boundary,
            ..
        } if *font == word.font => {
            if components.is_empty() && *left_boundary {
                word.original[0] = 256;
                (word.before_before, 0, Initial::default())
            } else {
                word.original[0] = (*character).into();
                (
                    word.before_before,
                    0,
                    Initial {
                        components: components.head(),
                        ligature: true,
                        left: *left_boundary,
                    },
                )
            }
        }
        Kind::Character { .. } | Kind::Ligature { .. } => {
            word.original[0] = 256;
            (word.before, 0, Initial::default())
        }
        _ => {
            let left = matches!(
                nodes.get(node.next).kind,
                Kind::Ligature {
                    left_boundary: true,
                    ..
                }
            );
            word.original[0] = 256;
            (word.before, usize::from(!left), Initial::default())
        }
    }
}

#[derive(Default)]
struct Branches {
    hold: List,
    major: List,
    pre: List,
    post: List,
}
impl Branches {
    fn release(&mut self, nodes: &mut NodeStore, scratch: &mut ReleaseScratch) {
        for list in [
            &mut self.hold,
            &mut self.major,
            &mut self.pre,
            &mut self.post,
        ] {
            nodes.release(std::mem::take(list), scratch);
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn rebuild(
    r: &mut Reconstituter<'_>,
    mut j: usize,
    n: usize,
    boundary: u16,
    hyphen: u16,
    mut positions: u64,
    output: &mut List,
    b: &mut Branches,
    release: &mut ReleaseScratch,
) -> Result<()> {
    while j <= n {
        let mut l = j;
        let (end, mut passed) = r.run(j, n, boundary, hyphen, positions, &mut b.hold)?;
        j = end + 1;
        if passed == 0 {
            r.nodes.splice(output, std::mem::take(&mut b.hold));
            if positions & (1u64 << (j - 1)) != 0 {
                l = j;
                passed = j - 1;
            }
        }
        if passed == 0 {
            continue;
        }
        loop {
            b.major = std::mem::take(&mut b.hold);
            let mut i = passed;
            positions &= !(1u64 << i);
            let has_hyphen = r.metrics.exists(hyphen as u8);
            let saved = if has_hyphen {
                i += 1;
                Some(std::mem::replace(&mut r.original[i], hyphen))
            } else {
                None
            };
            while l <= i {
                l = r
                    .run(l, i, r.resource.boundary, 256, positions, &mut b.pre)?
                    .0
                    + 1;
            }
            if let Some(saved) = saved {
                r.original[i] = saved;
                l = i;
                i -= 1;
            }
            debug_assert_eq!(l, i + 1);
            let mut saved = if r.resource.boundary_label.is_some() {
                l -= 1;
                Some((l, std::mem::replace(&mut r.original[l], 256)))
            } else {
                None
            };
            while l < j {
                loop {
                    l = r.run(l, n, boundary, 256, positions, &mut b.post)?.0 + 1;
                    if let Some((index, character)) = saved.take() {
                        r.original[index] = character;
                    }
                    if l >= j {
                        break;
                    }
                }
                while l > j {
                    j = r.run(j, n, boundary, 256, positions, &mut b.major)?.0 + 1;
                }
            }
            let mut count = 0usize;
            let mut p = b.major.head();
            while !p.is_null() {
                count += 1;
                p = r.nodes.get(p).next;
            }
            if count <= 127 {
                let disc = r.alloc(Kind::discretionary(
                    std::mem::take(&mut b.pre),
                    std::mem::take(&mut b.post),
                    count as u8,
                ))?;
                r.nodes.append(output, disc);
            } else {
                // TeX forgets only this discretionary; retain its replacement.
                // The caller's release scratch is separate from reconstruction.
                r.nodes.release(std::mem::take(&mut b.pre), release);
                r.nodes.release(std::mem::take(&mut b.post), release);
            }
            r.nodes.splice(output, std::mem::take(&mut b.major));
            passed = j - 1;
            if positions & (1u64 << passed) == 0 {
                break;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "hyphenation_render_tests.rs"]
mod tests;
