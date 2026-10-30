mod condition;
mod convert;
mod define;
mod delimit;
mod execute;
mod expand;
mod expression;
mod input;
#[cfg(feature = "lexing-prototype")]
#[doc(hidden)]
pub mod lexing;
mod lists;
mod macros;
mod names;
mod scalars;
mod scan;
mod token;
mod value_metadata;
mod values;
use names::Names;
use scalars::{Scalars, Target};
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
    Capacity,
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
    pub messages: Vec<Vec<u8>>,
}
pub struct Engine<'a> {
    pub(crate) sources: Vec<Source>,
    pub(crate) bindings: Vec<Binding>,
    pub(crate) values: values::Values,
    pub(crate) packets: Vec<Packet>,
    pub(crate) file: input::FileInput<'a>,
    pub(crate) names: Box<Names>,
    pub(crate) binding_owners: Vec<Option<values::Owner>>,
    pub(crate) scalars: Scalars,
    pub(crate) token_registers: [TokenValue; 256],
    pub(crate) after_assignment: Option<Token>,
    pub(crate) free_packets: [u32; PACKET_ARITIES],
    pub(crate) packet_links: Vec<u32>,
    pub(crate) name_spills: Vec<value_metadata::Buffer<u8>>,
    pub(crate) save: Vec<execute::Save>,
    pub(crate) level: u16,
    pub(crate) conditions: Vec<condition::Condition>,
    pub(crate) messages: Vec<Vec<u8>>,
    pub(crate) par: Token,
    pub(crate) expansion_depth: u16,
}
impl<'a> Engine<'a> {
    pub fn new(source: &'a [u8]) -> Self {
        let mut engine = Self {
            file: input::FileInput::new(source),
            names: Box::new(Names::new()),
            bindings: vec![Binding::default(); NAMED_BASE as usize],
            binding_owners: vec![None; NAMED_BASE as usize],
            scalars: Scalars::default(),
            token_registers: std::array::from_fn(|_| TokenValue::default()),
            after_assignment: None,
            values: values::Values::default(),
            sources: Vec::with_capacity(16),
            packets: Vec::new(),
            free_packets: [NO_PACKET; PACKET_ARITIES],
            packet_links: Vec::new(),
            name_spills: Vec::new(),
            save: Vec::new(),
            level: 0,
            conditions: Vec::new(),
            messages: Vec::new(),
            par: Token(0),
            expansion_depth: 0,
        };
        engine.bindings[0] = Binding::new(Meaning::RELAX, 0);
        for &(command, names) in cmd::PRIMITIVES {
            for (value, name) in names.iter().enumerate() {
                let id = engine
                    .intern(name)
                    .expect("primitive names fit the token encoding");
                engine.bindings[id as usize] = Binding::new(
                    Meaning {
                        command,
                        value: value as i32,
                    },
                    0,
                );
                if command == cmd::PAR {
                    engine.par = Token::cs(id);
                }
            }
        }
        engine
    }
    /// Supply the host's clock parameters before execution. The portable MVP
    /// starts with 00:00, January 1, 1970 until the host sets these values.
    pub fn set_calendar(&mut self, minutes: i32, day: i32, month: i32, year: i32) {
        for (index, value) in [minutes, day, month, year].into_iter().enumerate() {
            self.scalars[Target::parameter(index + 2)] = Integer { value, level: 0 };
        }
    }
    pub fn run(mut self) -> Result<Outcome> {
        self.run_input()?;
        Ok(Outcome {
            counts: self.scalars.counts().map(|i| i.value),
            messages: self.messages,
        })
    }
    #[inline(never)]
    fn run_input(&mut self) -> Result<()> {
        loop {
            let read = self.next_expanded()?;
            if read.meaning.command == cmd::END {
                if self.level != 0 {
                    return Err(Error::Group);
                }
                if !self.conditions.is_empty() {
                    return Err(Error::Conditional);
                }
                return Ok(());
            }
            self.execute(read)?;
        }
    }
    pub(crate) fn intern(&mut self, name: &[u8]) -> Result<u32> {
        let id = self.names.intern(name)?;
        if id as usize >= self.bindings.len() {
            self.grow_bindings(id as usize + 1);
        }
        Ok(id)
    }
    #[cold]
    #[inline(never)]
    pub(crate) fn grow_bindings(&mut self, len: usize) {
        self.bindings.resize(len, Binding::default());
        self.binding_owners.resize_with(len, || None);
    }
    #[inline(always)]
    pub(crate) fn resolve(&self, token: Token) -> Meaning {
        if token.is_cs() {
            self.bindings[token.id()].meaning()
        } else {
            Meaning {
                command: token.cat(),
                value: i32::from(token.byte()),
            }
        }
    }
    #[inline(always)]
    pub(crate) fn next_token(&mut self) -> Result<Read> {
        let raw = self.next_raw()?;
        let mut meaning = self.resolve(raw.token);
        if raw.suppress && meaning.expandable() {
            meaning = Meaning::NOEXPAND_RELAX;
        }
        Ok(Read {
            token: raw.token,
            meaning,
        })
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
