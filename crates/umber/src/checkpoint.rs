//! Outer-driver handoff and explicit semantic roots, including external modes.
use crate::{
    Document, Engine, Error, Outcome, Result,
    condition::Condition,
    execute::Save,
    input::FileInput,
    messages::Messages,
    scalars::Scalars,
    tables::{Bindings, Table},
    token::*,
    value_metadata::{Buffer, Store},
    values::Owner,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// No command or scanner is suspended; the current slice is checkpointable.
    Boundary,
    Complete,
}

#[derive(Clone)]
enum SavedSource {
    Inline {
        tokens: [Token; 2],
        position: u8,
        len: u8,
        suppress: bool,
    },
    Tokens {
        position: u32,
        store: u32,
        owner: Option<Owner>,
        argument: Option<(u32, u8)>,
        span_index: usize,
        span_end: u32,
        owns_packet: Option<u32>,
    },
}

/// Immutable roots for one session and unchanged source. May outlive its engine.
/// Restoring does not consume this checkpoint, so branches can be replayed.
pub struct Checkpoint<'a> {
    session: Rc<()>,
    sources: Buffer<SavedSource>,
    packets: Buffer<Packet>,
    bindings: Bindings,
    file: FileInput<'a>,
    scalars: Scalars,
    token_registers: Table<TokenValue>,
    layout_values: crate::layout_values::LayoutValues,
    nodes: crate::nodes::NodeStore,
    fonts: crate::fonts::Fonts,
    text: crate::text::TextState,
    page: crate::page::PageState,
    after_assignment: Option<Token>,
    save: Buffer<Save>,
    level: u16,
    conditions: Buffer<Condition>,
    messages: Messages,
    par: Token,
    complete: bool,
}

/// Buffers own their initialized records and retain these stores independently.
/// Keeping stores in the session permits checkpoint/drop/restore to reuse pages.
#[derive(Default)]
pub(crate) struct Stores {
    sources: Rc<Store<SavedSource>>,
    packets: Rc<Store<Packet>>,
    saves: Rc<Store<Save>>,
    conditions: Rc<Store<Condition>>,
    ids: Rc<Store<u32>>,
}

impl<'a> Engine<'a> {
    /// Whether the current outer-driver state can be retained. Ordinary
    /// assignment groups are allowed. Horizontal construction defers handoff
    /// until a successful return to external vertical mode.
    pub fn can_checkpoint(&self) -> bool {
        self.quiescent && self.document.text.mode == crate::text::Mode::Vertical
    }

    /// Execute up to `command_budget` complete outer commands. Expansion and
    /// operand scanning finish before a boundary is returned. Horizontal mode
    /// defers handoff until an eligible vertical boundary. Zero only polls.
    pub fn advance(&mut self, command_budget: usize) -> Result<Progress> {
        if !self.quiescent {
            return Err(Error::Checkpoint);
        }
        if command_budget > 0 {
            self.font_catalog.sealed = true;
        }
        if self.document.complete {
            return Ok(Progress::Complete);
        }
        let mut commands = 0usize;
        while commands < command_budget || !self.can_checkpoint() {
            self.quiescent = false;
            let read = self.next_expanded()?;
            if read.meaning.command == cmd::END {
                if self.document.text.mode == crate::text::Mode::Horizontal {
                    self.head_for_vertical(read);
                    self.quiescent = true;
                    commands = commands.saturating_add(1);
                    continue;
                }
                if self.document.level != 0 {
                    return Err(Error::Group);
                }
                if !self.document.conditions.is_empty() {
                    return Err(Error::Conditional);
                }
                if !self.end_pages(read)? {
                    self.quiescent = true;
                    commands = commands.saturating_add(1);
                    continue;
                }
                self.document.complete = true;
                self.quiescent = true;
                return Ok(Progress::Complete);
            }
            self.execute(read)?;
            // Scanners have returned, and all pending reads belong to sources.
            debug_assert_eq!(self.expansion_depth, 0);
            self.quiescent = true;
            commands = commands.saturating_add(1);
        }
        Ok(Progress::Boundary)
    }

    /// Publish register values and the current replacement message prefix.
    pub fn outcome(&self) -> Outcome {
        Outcome {
            counts: self.document.scalars.counts().map(|i| i.value),
            messages: self.document.messages.copy(),
            pages: self.document.page.output.clone(),
        }
    }

    /// Capture only at the outer driver in external vertical mode. Future
    /// construction/output nests must further restrict the eligibility predicate.
    /// The source is borrowed for the checkpoint lifetime, but the engine is not.
    /// A checkpoint can outlive the engine; only its original session can restore it.
    ///
    /// ```
    /// use umber::{Engine, Progress};
    /// let mut engine = Engine::new(br"\count0=7\relax\count0=9\relax\end");
    /// engine.advance(1)?;
    /// let saved = engine.checkpoint()?;
    /// while engine.advance(32)? != Progress::Complete {}
    /// assert_eq!(engine.outcome().counts[0], 9);
    /// engine.restore(&saved)?;
    /// assert_eq!(engine.outcome().counts[0], 7);
    /// # Ok::<(), umber::Error>(())
    /// ```
    pub fn checkpoint(&self) -> Result<Checkpoint<'a>> {
        if !self.can_checkpoint() {
            return Err(Error::Checkpoint);
        }
        let mut ids = Buffer::new(&self.checkpoint_stores.ids);
        let mut packets = Buffer::new(&self.checkpoint_stores.packets);
        for source in &self.document.sources {
            if let Some(id) = source.cursor().and_then(|c| c.owns_packet) {
                ids.push(id);
            }
        }
        // Packet IDs follow reusable per-arity slots, not source-stack order.
        // Independent copies use sorted IDs as checkpoint-local coordinates.
        ids.sort_unstable();
        for &id in &ids {
            packets.push(self.copy_packet(&self.packets[id as usize]));
        }
        let saved_id = |id| ids.binary_search(&id).expect("body owns argument packet") as u32;
        let mut sources = Buffer::new(&self.checkpoint_stores.sources);
        sources.extend(self.document.sources.iter().map(|source| match source {
            Source::Inline {
                tokens,
                position,
                len,
                suppress,
            } => SavedSource::Inline {
                tokens: *tokens,
                position: *position,
                len: *len,
                suppress: *suppress,
            },
            Source::Tokens(c) => {
                // SAFETY: the retained cursor window has initialized tokens and
                // both pointers belong to that same window, including one-past.
                let remaining = unsafe { c.end.offset_from(c.next) } as u32;
                SavedSource::Tokens {
                    position: c.end_position - remaining,
                    store: c.store,
                    owner: c._owner.clone(),
                    argument: c.argument.map(|(id, arg)| (saved_id(id), arg)),
                    span_index: c.span_index,
                    span_end: c.span_end,
                    owns_packet: c.owns_packet.map(saved_id),
                }
            }
        }));
        let d = &self.document;
        let mut save = Buffer::new(&self.checkpoint_stores.saves);
        save.extend(d.save.iter().cloned());
        let mut conditions = Buffer::new(&self.checkpoint_stores.conditions);
        conditions.extend_from_slice(&d.conditions);
        Ok(Checkpoint {
            session: self.session.clone(),
            sources,
            packets,
            bindings: d.bindings.clone(),
            file: d.file.clone(),
            scalars: d.scalars.clone(),
            token_registers: d.token_registers.clone(),
            layout_values: d.layout_values.clone(),
            nodes: d.nodes.clone(),
            fonts: d.fonts.clone(),
            text: d.text.snapshot(),
            page: d.page.snapshot(),
            after_assignment: d.after_assignment,
            save,
            level: d.level,
            conditions,
            messages: d.messages.clone(),
            par: d.par,
            complete: d.complete,
        })
    }

    fn copy_packet(&self, source: &Packet) -> Packet {
        let mut packet = self.values.packet();
        self.values
            .prepare_packet(&mut packet, source.arguments.len());
        Self::copy_arguments(source, &mut packet);
        packet
    }
    fn copy_arguments(source: &Packet, destination: &mut Packet) {
        for (from, to) in source
            .arguments
            .iter()
            .zip(destination.arguments.iter_mut())
        {
            to.first = from.first;
            to.rest.extend_from_slice(&from.rest);
        }
        destination.owners.extend(source.owners.iter().cloned());
    }

    /// Replace the document roots, keeping session names, stores and capacities.
    /// A foreign checkpoint is rejected before any live state is changed.
    pub fn restore(&mut self, checkpoint: &Checkpoint<'a>) -> Result<()> {
        if !Rc::ptr_eq(&self.session, &checkpoint.session) {
            return Err(Error::Checkpoint);
        }
        self.quiescent = false;
        // Sources are discarded before packets; no borrowed argument window is
        // read after this point. Clear every packet, including scanner failures
        // that never installed a body owner, and rebuild only allocator lists.
        self.document.sources.clear();
        self.free_packets.fill(NO_PACKET);
        for (id, packet) in self.packets.iter_mut().enumerate() {
            packet.clear();
            let count = packet.arguments.len();
            self.packet_links[id] = self.free_packets[count];
            self.free_packets[count] = id as u32;
        }
        self.expansion_depth = 0;
        let mut ids = Buffer::new(&self.checkpoint_stores.ids);
        for packet in checkpoint.packets.iter() {
            let id = self.take_packet(Arity::new(packet.arguments.len() as u8))?;
            Self::copy_arguments(packet, &mut self.packets[id as usize]);
            ids.push(id);
        }
        for source in checkpoint.sources.iter() {
            let source = match source {
                SavedSource::Inline {
                    tokens,
                    position,
                    len,
                    suppress,
                } => Source::Inline {
                    tokens: *tokens,
                    position: *position,
                    len: *len,
                    suppress: *suppress,
                },
                SavedSource::Tokens {
                    position,
                    store,
                    owner,
                    argument,
                    span_index,
                    span_end,
                    owns_packet,
                } => {
                    let span = Span {
                        store: *store,
                        start: *position,
                        end: *span_end,
                    };
                    let (next, end, end_position) = if position == span_end {
                        // Keep an exhausted nonempty span on its final valid
                        // one-past window; the ordinary reader retires it.
                        let (_, end, end_position) = self.values.window(span, position - 1);
                        (end, end, end_position)
                    } else {
                        self.values.window(span, *position)
                    };
                    Source::Tokens(Cursor {
                        next,
                        end,
                        end_position,
                        store: *store,
                        _owner: owner.clone(),
                        argument: argument.map(|(id, arg)| (ids[id as usize], arg)),
                        span_index: *span_index,
                        span_end: *span_end,
                        owns_packet: owns_packet.map(|id| ids[id as usize]),
                    })
                }
            };
            self.document.sources.push(source);
        }
        let Document {
            bindings,
            file,
            scalars,
            token_registers,
            layout_values,
            nodes,
            fonts,
            text,
            page,
            after_assignment,
            save,
            level,
            conditions,
            messages,
            par,
            complete,
            sources: _,
        } = &mut self.document;
        bindings.clone_from(&checkpoint.bindings);
        file.clone_from(&checkpoint.file);
        scalars.clone_from(&checkpoint.scalars);
        token_registers.clone_from(&checkpoint.token_registers);
        layout_values.clone_from(&checkpoint.layout_values);
        nodes.clone_from(&checkpoint.nodes);
        fonts.clone_from(&checkpoint.fonts);
        *text = checkpoint.text.snapshot();
        *page = checkpoint.page.snapshot();
        *after_assignment = checkpoint.after_assignment;
        save.clear();
        save.extend(checkpoint.save.iter().cloned());
        *level = checkpoint.level;
        conditions.clear();
        conditions.extend_from_slice(&checkpoint.conditions);
        messages.clone_from(&checkpoint.messages);
        *par = checkpoint.par;
        *complete = checkpoint.complete;
        self.quiescent = true;
        Ok(())
    }
}

#[cfg(test)]
#[path = "checkpoint_tests.rs"]
mod tests;
