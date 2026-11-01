mod checkpoint;
mod condition;
mod convert;
mod define;
mod delimit;
mod dimension_scan;
mod execute;
mod expand;
mod expression;
mod fonts;
mod input;
mod layout_assign;
mod layout_values;
#[cfg(feature = "lexing-prototype")]
#[doc(hidden)]
pub mod lexing;
mod ligatures;
mod line_break;
mod line_construct;
mod line_search;
mod lists;
mod macros;
mod messages;
mod names;
#[allow(dead_code)] // Node consumers are the following font/paragraph/page slices.
mod nodes;
mod numeric;
mod output;
mod packing;
mod page;
mod page_output;
mod paragraph;
mod quantities;
mod scalars;
mod scan;
mod scanner;
mod tables;
mod text;
mod tfm;
mod token;
mod value_metadata;
mod values;
mod vertical;
mod write;
pub use checkpoint::{Checkpoint, Progress};
pub use messages::{Diagnostic, DiagnosticDestination};
use names::Names;
pub use output::{GlueSetting, OutputItem, PackedBox, Page, Pages, Placement, Placements};
use scalars::{Scalars, Target};
pub use tfm::{FontResource, GlyphMetrics};
use token::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Eof,
    Undefined,
    Unsupported,
    Syntax,
    Number,
    Overflow,
    Group,
    Conditional,
    Argument,
    Outer,
    Capacity,
    Checkpoint,
    Font,
    Resource,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub(crate) type Result<T> = std::result::Result<T, Error>;
pub struct Outcome {
    pub counts: [i32; 256],
    pub diagnostics: Vec<Diagnostic>,
    pub pages: Pages,
}
#[cfg(test)]
impl Outcome {
    pub(crate) fn message_bytes(&self) -> Vec<Vec<u8>> {
        self.diagnostics
            .iter()
            .filter_map(|diagnostic| match diagnostic {
                Diagnostic::Message(bytes) => Some(bytes.clone()),
                Diagnostic::Write { .. } => None,
            })
            .collect()
    }
}
/// A source-bound session. Names, token stores and reusable scratch survive restore.
pub struct Engine<'a> {
    pub(crate) document: Document<'a>,
    pub(crate) values: values::Values,
    pub(crate) packets: Vec<Packet>,
    pub(crate) names: Box<Names>,
    pub(crate) free_packets: [u32; PACKET_ARITIES],
    pub(crate) packet_links: Vec<u32>,
    pub(crate) name_spills: Vec<value_metadata::Buffer<u8>>,
    pub(crate) expansion_depth: u16,
    pub(crate) scanner: scanner::Scanner,
    pub(crate) font_catalog: tfm::Catalog,
    font_name: Vec<u8>,
    ligatures: ligatures::Scratch,
    paragraph: paragraph::Scratch,
    page_lowering: Option<page_output::Scratch>,
    page_release: nodes::ReleaseScratch,
    output_pools: output::Pools,
    checkpoint_stores: checkpoint::Stores,
    session: std::rc::Rc<()>,
    quiescent: bool,
}

/// Every live semantic root belongs here; reusable stores do not.
pub(crate) struct Document<'a> {
    pub sources: Vec<Source>,
    pub bindings: tables::Bindings,
    pub file: input::FileInput<'a>,
    pub scalars: Scalars,
    pub token_registers: tables::Table<TokenValue>,
    pub layout_values: layout_values::LayoutValues,
    pub nodes: nodes::NodeStore,
    pub fonts: fonts::Fonts,
    pub text: text::TextState,
    pub page: page::PageState,
    pub after_assignment: Option<Token>,
    pub save: Vec<execute::Save>,
    pub level: u16,
    pub conditions: Vec<condition::Condition>,
    pub diagnostics: messages::Diagnostics,
    pub par: Token,
    pub complete: bool,
}
impl<'a> Engine<'a> {
    pub fn new(source: &'a [u8]) -> Self {
        let mut engine = Self {
            document: Document {
                file: input::FileInput::new(source),
                bindings: tables::Bindings::new(NAMED_BASE as usize),
                scalars: Scalars::default(),
                token_registers: tables::Table::new(256),
                layout_values: layout_values::LayoutValues::default(),
                nodes: nodes::NodeStore::default(),
                fonts: fonts::Fonts::default(),
                text: text::TextState::default(),
                page: page::PageState::default(),
                after_assignment: None,
                sources: Vec::with_capacity(16),
                save: Vec::new(),
                level: 0,
                conditions: Vec::new(),
                diagnostics: messages::Diagnostics::default(),
                par: Token(0),
                complete: false,
            },
            names: Box::new(Names::new()),
            values: values::Values::default(),
            packets: Vec::new(),
            free_packets: [NO_PACKET; PACKET_ARITIES],
            packet_links: Vec::new(),
            name_spills: Vec::new(),
            expansion_depth: 0,
            scanner: scanner::Scanner::Normal,
            font_catalog: tfm::Catalog::default(),
            font_name: Vec::new(),
            ligatures: ligatures::Scratch::default(),
            paragraph: paragraph::Scratch::default(),
            page_lowering: Some(page_output::Scratch::default()),
            page_release: nodes::ReleaseScratch::default(),
            output_pools: output::Pools::default(),
            checkpoint_stores: checkpoint::Stores::default(),
            session: std::rc::Rc::new(()),
            quiescent: true,
        };
        engine.document.bindings[0] = Binding::new(Meaning::RELAX, 0);
        engine.document.bindings[FROZEN_END_WRITE.id()] = Binding::new(
            Meaning {
                command: cmd::END_WRITE,
                value: 0,
            },
            0,
        );
        for &(command, names) in cmd::PRIMITIVES {
            for (value, name) in names.iter().enumerate() {
                let id = engine
                    .intern(name)
                    .expect("primitive names fit the token encoding");
                engine.document.bindings[id as usize] = Binding::new(
                    Meaning {
                        command,
                        value: value as i32,
                    },
                    0,
                );
                if command == cmd::PAR {
                    engine.document.par = Token::cs(id);
                }
                if command == cmd::SELECT_FONT {
                    engine.document.fonts.descriptors[0].identifier = id as usize;
                }
            }
        }
        engine
    }
    /// Supply the host's clock parameters before execution. The portable MVP
    /// starts with 00:00, January 1, 1970 until the host sets these values.
    pub fn set_calendar(&mut self, minutes: i32, day: i32, month: i32, year: i32) {
        for (index, value) in [minutes, day, month, year].into_iter().enumerate() {
            self.document.scalars[Target::parameter(index + 2)] = Integer { value, level: 0 };
        }
    }
    pub fn run(mut self) -> Result<Outcome> {
        self.run_input()?;
        Ok(Outcome {
            counts: self.document.scalars.counts().map(|i| i.value),
            diagnostics: self.document.diagnostics.into_vec(),
            pages: self.document.page.output,
        })
    }
    #[inline(never)]
    fn run_input(&mut self) -> Result<()> {
        while self.advance(usize::MAX)? != Progress::Complete {}
        Ok(())
    }
    pub(crate) fn intern(&mut self, name: &[u8]) -> Result<u32> {
        let id = self.names.intern(name)?;
        if id as usize >= self.document.bindings.len() {
            self.grow_bindings(id as usize + 1);
        }
        Ok(id)
    }
    #[cold]
    #[inline(never)]
    pub(crate) fn grow_bindings(&mut self, len: usize) {
        self.document.bindings.grow(len);
    }
    #[inline(always)]
    pub(crate) fn resolve(&self, token: Token) -> Meaning {
        if token.is_cs() {
            if let Some(font) = token.font_id() {
                Meaning {
                    command: cmd::SELECT_FONT,
                    value: font as i32,
                }
            } else {
                self.document.bindings[token.id()].meaning()
            }
        } else {
            Meaning {
                command: token.cat(),
                value: i32::from(token.byte()),
            }
        }
    }
    #[inline(always)]
    pub(crate) fn raw_meaning(&self, raw: Raw) -> Result<Meaning> {
        let mut meaning = self.resolve(raw.token);
        if raw.suppress && meaning.expandable() {
            meaning = Meaning::NOEXPAND_RELAX;
        } else {
            self.check_outer(meaning)?;
        }
        Ok(meaning)
    }
    #[inline(always)]
    pub(crate) fn next_expanded(&mut self) -> Result<Read> {
        loop {
            let read = self.next_token()?;
            if !read.meaning.expandable() {
                return Ok(read);
            }
            if read.meaning.is_macro() {
                self.begin_macro(read.meaning.value as u32)?;
            } else {
                self.expand(read)?;
            }
        }
    }
}
#[cfg(test)]
mod extended_tests;
#[cfg(test)]
mod scan_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod memory_tests;

#[cfg(test)]
mod dimension_tests;
#[cfg(test)]
mod math_code_tests;
#[cfg(test)]
mod math_glue_tests;
#[cfg(test)]
mod parameter_tests;

#[cfg(test)]
mod font_state_tests;
#[cfg(test)]
mod font_test_data;
#[cfg(test)]
mod font_text_tests;

#[cfg(test)]
mod font_oracle_tests;

#[cfg(test)]
mod paragraph_oracle_tests;
#[cfg(test)]
mod paragraph_tests;

#[cfg(test)]
mod page_tests;

#[cfg(test)]
mod page_dvi_tests;
#[cfg(test)]
mod page_oracle_tests;
