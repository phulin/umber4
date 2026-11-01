//! Document-owned hyphenation tables; snapshots retain two semantic roots.
use crate::{Error, Result, tables::Table};
use std::rc::Rc;

#[derive(Clone, Copy, Default)]
struct TrieNode {
    child: u32,
    next: u32,
    operations: u32,
    character: u8,
    count: u8,
}
#[derive(Clone, Copy, Default)]
struct Operation {
    position: u8,
    weight: u8,
}
#[derive(Clone)]
struct Patterns {
    nodes: Table<TrieNode>,
    operations: Table<Operation>,
}
impl Default for Patterns {
    fn default() -> Self {
        Self {
            nodes: Table::new(1),
            operations: Table::new(0),
        }
    }
}
impl Patterns {
    fn child(&self, parent: u32, character: u8) -> u32 {
        let mut node = self.nodes[parent as usize].child;
        while node != 0 && self.nodes[node as usize].character < character {
            node = self.nodes[node as usize].next;
        }
        if node != 0 && self.nodes[node as usize].character == character {
            node
        } else {
            0
        }
    }
    fn insert_child(&mut self, parent: u32, character: u8) -> Result<u32> {
        let mut previous = 0;
        let mut next = self.nodes[parent as usize].child;
        while next != 0 && self.nodes[next as usize].character < character {
            previous = next;
            next = self.nodes[next as usize].next;
        }
        if next != 0 && self.nodes[next as usize].character == character {
            return Ok(next);
        }
        let index = self.nodes.len();
        let id = u32::try_from(index).map_err(|_| Error::Capacity)?;
        self.nodes[index] = TrieNode {
            next,
            character,
            ..TrieNode::default()
        };
        if previous == 0 {
            self.nodes[parent as usize].child = id;
        } else {
            self.nodes[previous as usize].next = id;
        }
        Ok(id)
    }
    fn insert(&mut self, language: u8, letters: &[u8], weights: &[u8]) -> Result<()> {
        let start = u32::try_from(self.operations.len()).map_err(|_| Error::Capacity)?;
        let count = weights.iter().filter(|&&weight| weight != 0).count();
        start.checked_add(count as u32).ok_or(Error::Capacity)?;
        // TeX prepares operations before inserting trie edges and testing for a
        // duplicate. On failure earlier completed entries remain published.
        for (position, &weight) in weights.iter().enumerate() {
            if weight != 0 {
                let index = self.operations.len();
                self.operations[index] = Operation {
                    position: position as u8,
                    weight,
                };
            }
        }
        let mut node = self.insert_child(0, language)?;
        for &character in letters {
            node = self.insert_child(node, character)?;
        }
        let terminal = &mut self.nodes[node as usize];
        // All-zero patterns do not mark a terminal as set in TeX.
        if terminal.count != 0 {
            return Err(Error::Syntax);
        }
        terminal.operations = start;
        terminal.count = count as u8;
        Ok(())
    }
    fn weights(&self, language: u8, word: &[u8], weights: &mut [u8; 65]) {
        let root = self.child(0, language);
        if root == 0 {
            return;
        }
        let mut padded = [0; 65];
        padded[1..word.len() + 1].copy_from_slice(word);
        for start in 0..=word.len() {
            let mut node = root;
            for &character in &padded[start..word.len() + 2] {
                node = self.child(node, character);
                if node == 0 {
                    break;
                }
                let terminal = self.nodes[node as usize];
                for i in 0..u32::from(terminal.count) {
                    let operation = self.operations[(terminal.operations + i) as usize];
                    // Padded position 1 is the first letter, so a weight at
                    // padded offset p describes the gap after p-1 letters.
                    let position = start + operation.position as usize;
                    if position > 0 && position <= word.len() + 1 {
                        weights[position - 1] = weights[position - 1].max(operation.weight);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Exception {
    word: [u8; 63],
    length: u8,
    language: u8,
    next: u32,
    positions: u64,
}
impl Default for Exception {
    fn default() -> Self {
        Self {
            word: [0; 63],
            length: 0,
            language: 0,
            next: 0,
            positions: 0,
        }
    }
}
#[derive(Clone)]
struct Exceptions {
    entries: Table<Exception>,
    buckets: Table<u32>,
}
impl Default for Exceptions {
    fn default() -> Self {
        Self {
            entries: Table::new(1),
            buckets: Table::new(8),
        }
    }
}
impl Exceptions {
    fn bucket(&self, language: u8, word: &[u8]) -> usize {
        let hash = word
            .iter()
            .fold(2_166_136_261u32 ^ u32::from(language), |h, &c| {
                (h ^ u32::from(c)).wrapping_mul(16_777_619)
            });
        hash as usize & (self.buckets.len() - 1)
    }
    fn find(&self, language: u8, word: &[u8]) -> u32 {
        let mut id = self.buckets[self.bucket(language, word)];
        while id != 0 {
            let entry = &self.entries[id as usize];
            if entry.language == language && &entry.word[..entry.length as usize] == word {
                return id;
            }
            id = entry.next;
        }
        0
    }
    fn insert(&mut self, language: u8, word: &[u8], positions: u64) -> Result<()> {
        let old = self.find(language, word);
        if old != 0 {
            self.entries[old as usize].positions = positions;
            return Ok(());
        }
        let index = self.entries.len();
        let id = u32::try_from(index).map_err(|_| Error::Capacity)?;
        // Keep the load at most 3/4; no fixed dictionary fills with long chains
        // as a document adds exceptions. Existing keys retain their IDs.
        if index > self.buckets.len() / 4 * 3 {
            let count = self.buckets.len().checked_mul(2).ok_or(Error::Capacity)?;
            u32::try_from(count).map_err(|_| Error::Capacity)?;
            self.buckets.grow(count);
            for bucket in 0..count {
                self.buckets[bucket] = 0;
            }
            for i in 1..index {
                let entry = self.entries[i];
                let bucket = self.bucket(entry.language, &entry.word[..entry.length as usize]);
                self.entries[i].next = self.buckets[bucket];
                self.buckets[bucket] = i as u32;
            }
        }
        let bucket = self.bucket(language, word);
        let mut entry = Exception {
            length: word.len() as u8,
            language,
            next: self.buckets[bucket],
            positions,
            ..Exception::default()
        };
        entry.word[..word.len()].copy_from_slice(word);
        self.entries[index] = entry;
        self.buckets[bucket] = id;
        Ok(())
    }
}

#[derive(Clone, Default)]
pub(crate) struct Hyphenation {
    patterns: Option<Rc<Patterns>>,
    exceptions: Option<Rc<Exceptions>>,
    frozen: bool,
}
impl Hyphenation {
    pub fn begin_pass(&mut self) -> Result<()> {
        self.frozen = true;
        Ok(())
    }
    pub fn check_patterns(&self) -> Result<()> {
        if self.frozen {
            Err(Error::Syntax)
        } else {
            Ok(())
        }
    }
    pub fn pattern(&mut self, language: u8, letters: &[u8], weights: &[u8]) -> Result<()> {
        let patterns = self
            .patterns
            .get_or_insert_with(|| Rc::new(Patterns::default()));
        Rc::make_mut(patterns).insert(language, letters, weights)
    }
    pub fn exception(&mut self, language: u8, word: &[u8], positions: u64) -> Result<()> {
        let exceptions = self
            .exceptions
            .get_or_insert_with(|| Rc::new(Exceptions::default()));
        Rc::make_mut(exceptions).insert(language, word, positions)
    }
    // Paragraph rendering supplies eligibility and original-to-lowercase
    // mapping. This lookup deliberately accepts only the bounded lowered word.
    pub fn positions(&self, language: u8, word: &[u8], left: i32, right: i32) -> u64 {
        let left = left.clamp(1, 63) as usize;
        let right = right.clamp(1, 63) as usize;
        if word.len() > 63 || word.len() < left + right {
            return 0;
        }
        let mut weights = [0; 65];
        let exception = self.exceptions.as_ref().and_then(|exceptions| {
            let id = exceptions.find(language, word);
            (id != 0).then(|| exceptions.entries[id as usize].positions)
        });
        if exception.is_none()
            && let Some(patterns) = &self.patterns
        {
            patterns.weights(language, word, &mut weights);
        }
        let mut positions = 0;
        for (i, &weight) in weights
            .iter()
            .enumerate()
            .take(word.len() - right + 1)
            .skip(left)
        {
            let bit = 1u64 << i;
            if exception.map_or(weight & 1 != 0, |mask| mask & bit != 0) {
                positions |= bit;
            }
        }
        positions
    }
}

#[cfg(test)]
#[path = "hyphenation_oracle_tests.rs"]
mod oracle_tests;
#[cfg(test)]
#[path = "hyphenation_tests.rs"]
mod tests;
