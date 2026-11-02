//! Checked logical trie/operation records and language-qualified exceptions.
use super::*;
use crate::format::wire::{Reader, Writer};

pub(crate) fn write(h: &Hyphenation, w: &mut Writer) -> Result<()> {
    if h.patterns.is_none() && h.exceptions.is_none() {
        w.u8(0);
        return Ok(());
    }
    w.u8(1);
    w.count(h.patterns.as_ref().map_or(0, |p| p.nodes.len()))?;
    if let Some(p) = &h.patterns {
        for i in 0..p.nodes.len() {
            let n = p.nodes[i];
            w.u32(n.child);
            w.u32(n.next);
            w.u32(n.operations);
            w.u8(n.character);
            w.u8(n.count);
        }
    }
    w.count(h.patterns.as_ref().map_or(0, |p| p.operations.len()))?;
    if let Some(p) = &h.patterns {
        for i in 0..p.operations.len() {
            w.u8(p.operations[i].position);
            w.u8(p.operations[i].weight);
        }
    }
    w.count(h.exceptions.as_ref().map_or(0, |e| e.entries.len() - 1))?;
    if let Some(e) = &h.exceptions {
        for i in 1..e.entries.len() {
            let entry = e.entries[i];
            w.u8(entry.language);
            w.blob(&entry.word[..entry.length as usize])?;
            w.u64(entry.positions);
        }
    }
    Ok(())
}

pub(crate) fn read(r: &mut Reader<'_>) -> Result<Hyphenation> {
    match r.u8()? {
        0 => {
            return Ok(Hyphenation {
                frozen: true,
                ..Hyphenation::default()
            });
        }
        1 => {}
        _ => return Err(Error::Format),
    }
    let nodes = r.count(14)?;
    let mut patterns = Patterns {
        nodes: Table::new(nodes),
        operations: Table::new(0),
    };
    for i in 0..nodes {
        patterns.nodes[i] = TrieNode {
            child: r.u32()?,
            next: r.u32()?,
            operations: r.u32()?,
            character: r.u8()?,
            count: r.u8()?,
        };
    }
    let operations = r.count(2)?;
    for i in 0..operations {
        let operation = Operation {
            position: r.u8()?,
            weight: r.u8()?,
        };
        if operation.position > 63 || !(1..=9).contains(&operation.weight) {
            return Err(Error::Format);
        }
        patterns.operations[i] = operation;
    }
    validate(&patterns)?;
    let count = r.count(15)?;
    let mut exceptions = (count != 0).then(Exceptions::default);
    for _ in 0..count {
        let language = r.u8()?;
        let word = r.blob()?;
        let positions = r.u64()?;
        if !(2..=63).contains(&word.len())
            || word.contains(&0)
            || positions >> (word.len().min(62) + 1) != 0
        {
            return Err(Error::Format);
        }
        let e = exceptions.as_mut().unwrap();
        if e.find(language, word) != 0 {
            return Err(Error::Format);
        }
        e.insert(language, word, positions)?;
    }
    Ok(Hyphenation {
        patterns: (nodes != 0).then(|| Rc::new(patterns)),
        exceptions: exceptions.map(Rc::new),
        frozen: true,
    })
}

fn validate(p: &Patterns) -> Result<()> {
    if p.nodes.len() == 0 {
        return if p.operations.len() == 0 {
            Ok(())
        } else {
            Err(Error::Format)
        };
    }
    let root = p.nodes[0];
    if root.next != 0 || root.operations != 0 || root.character != 0 || root.count != 0 {
        return Err(Error::Format);
    }
    // This one file-ID bitmap also rejects multiply owned paths and cycles.
    // Recursion is bounded by one language node plus 63 pattern characters.
    let mut seen = vec![false; p.nodes.len()];
    seen[0] = true;
    siblings(p, root.child, 1, &mut seen)?;
    if seen.contains(&false) {
        return Err(Error::Format);
    }
    Ok(())
}
fn siblings(p: &Patterns, mut id: u32, depth: usize, seen: &mut [bool]) -> Result<()> {
    let mut previous = None;
    while id != 0 {
        let index = id as usize;
        if depth > 64 || index >= seen.len() || seen[index] {
            return Err(Error::Format);
        }
        seen[index] = true;
        let node = p.nodes[index];
        if previous.is_some_and(|character| character >= node.character)
            || (depth == 1 && node.count != 0)
            || node.count > 64
        {
            return Err(Error::Format);
        }
        previous = Some(node.character);
        let start = node.operations as usize;
        let end = start
            .checked_add(node.count as usize)
            .ok_or(Error::Format)?;
        if end > p.operations.len() {
            return Err(Error::Format);
        }
        let mut position = None;
        for i in start..end {
            let op = p.operations[i];
            if op.position as usize >= depth
                || position.is_some_and(|previous| previous >= op.position)
            {
                return Err(Error::Format);
            }
            position = Some(op.position);
        }
        siblings(p, node.child, depth + 1, seen)?;
        id = node.next;
    }
    Ok(())
}

#[cfg(test)]
#[path = "hyphenation_format_tests.rs"]
mod tests;
