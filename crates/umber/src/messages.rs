//! Checkpoints share chronological diagnostic records in immutable append blocks.
use crate::value_metadata::{Buffer, Store};
use std::{cell::RefCell, rc::Rc};

const BLOCK_MESSAGES: usize = 16;
const CACHED: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticDestination {
    LogOnly,
    TerminalAndLog,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diagnostic {
    Message(Vec<u8>),
    Write {
        destination: DiagnosticDestination,
        bytes: Vec<u8>,
    },
}
impl Diagnostic {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::Message(bytes) | Self::Write { bytes, .. } => bytes,
        }
    }
    pub fn destination(&self) -> DiagnosticDestination {
        match self {
            Self::Message(_) => DiagnosticDestination::TerminalAndLog,
            Self::Write { destination, .. } => *destination,
        }
    }
}

#[derive(Default)]
struct Pool {
    messages: Rc<Store<Diagnostic>>,
    free: RefCell<Vec<Rc<Buffer<Diagnostic>>>>,
}
#[derive(Clone)]
struct Block {
    data: Option<Rc<Buffer<Diagnostic>>>,
    pool: Rc<Pool>,
}
impl Drop for Block {
    fn drop(&mut self) {
        let mut data = self.data.take().unwrap();
        if let Some(messages) = Rc::get_mut(&mut data) {
            messages.clear();
            let mut free = self.pool.free.borrow_mut();
            if free.len() < CACHED {
                free.push(data);
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct Diagnostics {
    blocks: Buffer<Block>,
    pool: Rc<Pool>,
}
impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            blocks: Buffer::new(&Rc::default()),
            pool: Rc::default(),
        }
    }
}
impl Diagnostics {
    pub fn push(&mut self, message: Diagnostic) {
        if let Some(block) = self.blocks.last_mut()
            && let Some(messages) = Rc::get_mut(block.data.as_mut().unwrap())
            && messages.len() < BLOCK_MESSAGES
        {
            messages.push(message);
            return;
        }
        let mut data = self.pool.free.borrow_mut().pop().unwrap_or_else(|| {
            let mut messages = Buffer::new(&self.pool.messages);
            messages.reserve(BLOCK_MESSAGES);
            Rc::new(messages)
        });
        Rc::get_mut(&mut data).unwrap().push(message);
        self.blocks.push(Block {
            data: Some(data),
            pool: self.pool.clone(),
        });
    }
    pub fn copy(&self) -> Vec<Diagnostic> {
        self.blocks
            .iter()
            .flat_map(|b| b.data.as_ref().unwrap().iter().cloned())
            .collect()
    }
    pub fn into_vec(mut self) -> Vec<Diagnostic> {
        let mut result = Vec::new();
        while let Some(mut block) = self.blocks.pop() {
            if let Some(messages) = Rc::get_mut(block.data.as_mut().unwrap()) {
                while let Some(message) = messages.pop() {
                    result.push(message);
                }
            } else {
                result.extend(block.data.as_ref().unwrap().iter().rev().cloned());
            }
        }
        result.reverse();
        result
    }
}

#[cfg(test)]
#[path = "messages_tests.rs"]
mod tests;
