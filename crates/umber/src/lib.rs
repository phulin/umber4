mod condition;
mod convert;
mod define;
mod execute;
mod expand;
mod expression;
mod input;
mod lists;
mod macros;
mod scalars;
mod scan;
mod token;
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
    pub(crate) file: input::FileInput<'a>,
    pub(crate) names: Names,
    pub(crate) bindings: Vec<Binding>,
    pub(crate) scalars: Scalars,
    pub(crate) token_registers: [TokenValue; 256],
    pub(crate) after_assignment: Option<Token>,
    pub(crate) arena: TokenArena,
    pub(crate) definitions: Vec<Definition>,
    pub(crate) sources: Vec<Source>,
    pub(crate) captures: Vec<Capture>,
    pub(crate) packets: Vec<Packet>,
    pub(crate) free_packets: Vec<u32>,
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
            names: Names::new(),
            bindings: vec![Binding::default(); 514],
            scalars: Scalars::default(),
            token_registers: [TokenValue::default(); 256],
            after_assignment: None,
            arena: TokenArena::default(),
            definitions: Vec::new(),
            sources: Vec::with_capacity(16),
            captures: Vec::new(),
            packets: Vec::new(),
            free_packets: Vec::new(),
            save: Vec::new(),
            level: 0,
            conditions: Vec::new(),
            messages: Vec::new(),
            par: Token(0),
            expansion_depth: 0,
        };
        let primitives: &[(&[u8], u8, i32)] = &[
            (b"relax", cmd::RELAX, 0),
            (b"def", cmd::DEF, 0),
            (b"gdef", cmd::GDEF, 0),
            (b"edef", cmd::EDEF, 0),
            (b"xdef", cmd::XDEF, 0),
            (b"let", cmd::LET, 0),
            (b"global", cmd::GLOBAL, 0),
            (b"long", cmd::LONG, 0),
            (b"begingroup", cmd::BEGIN_GROUP, 0),
            (b"endgroup", cmd::END_GROUP, 0),
            (b"count", cmd::COUNT, 0),
            (b"countdef", cmd::COUNT_DEF, 0),
            (b"advance", cmd::ARITH, 0),
            (b"multiply", cmd::ARITH, 1),
            (b"divide", cmd::ARITH, 2),
            (b"catcode", cmd::CATCODE, 0),
            (b"endlinechar", cmd::ENDLINE, 0),
            (b"message", cmd::MESSAGE, 0),
            (b"number", cmd::NUMBER, 0),
            (b"the", cmd::THE, 0),
            (b"expandafter", cmd::EXPANDAFTER, 0),
            (b"noexpand", cmd::NOEXPAND, 0),
            (b"csname", cmd::CSNAME, 0),
            (b"endcsname", cmd::ENDCSNAME, 0),
            (b"ifnum", cmd::IFNUM, 0),
            (b"ifx", cmd::IFX, 0),
            (b"iftrue", cmd::IFTRUE, 0),
            (b"iffalse", cmd::IFFALSE, 0),
            (b"else", cmd::ELSE, 0),
            (b"fi", cmd::FI, 0),
            (b"end", cmd::END, 0),
            (b"par", cmd::PAR, 0),
            (b"toks", cmd::TOKS, 0),
            (b"toksdef", cmd::TOKS_DEF, 0),
            (b"chardef", cmd::CHAR_DEF, 0),
            (b"futurelet", cmd::FUTURELET, 0),
            (b"afterassignment", cmd::AFTER_ASSIGN, 0),
            (b"aftergroup", cmd::AFTER_GROUP, 0),
            (b"globaldefs", cmd::INTEGER_PARAM, 0),
            (b"escapechar", cmd::INTEGER_PARAM, 1),
            (b"time", cmd::INTEGER_PARAM, 2),
            (b"day", cmd::INTEGER_PARAM, 3),
            (b"month", cmd::INTEGER_PARAM, 4),
            (b"year", cmd::INTEGER_PARAM, 5),
            (b"lccode", cmd::CASE_CODE, 0),
            (b"uccode", cmd::CASE_CODE, 1),
            (b"lowercase", cmd::CHANGE_CASE, 0),
            (b"uppercase", cmd::CHANGE_CASE, 1),
            (b"protected", cmd::PROTECTED, 0),
            (b"numexpr", cmd::NUMEXPR, 0),
            (b"string", cmd::STRING, 0),
            (b"meaning", cmd::MEANING, 0),
            (b"romannumeral", cmd::ROMAN, 0),
            (b"detokenize", cmd::DETOKENIZE, 0),
            (b"unexpanded", cmd::UNEXPANDED, 0),
            (b"if", cmd::IF, 0),
            (b"ifcat", cmd::IFCAT, 0),
            (b"ifodd", cmd::IFODD, 0),
            (b"ifcase", cmd::IFCASE, 0),
            (b"or", cmd::OR, 0),
            (b"ifdefined", cmd::IFDEFINED, 0),
            (b"ifcsname", cmd::IFCSNAME, 0),
            (b"unless", cmd::UNLESS, 0),
        ];
        engine.bindings[0] = Binding::new(Meaning::RELAX, 0);
        for &(name, command, value) in primitives {
            let id = engine
                .intern(name)
                .expect("primitive names fit the token encoding");
            engine.bindings[id as usize] = Binding::new(Meaning { command, value }, 0);
            if command == cmd::PAR {
                engine.par = Token::cs(id);
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
        loop {
            let read = self.next_expanded()?;
            if read.meaning.command == cmd::END {
                if self.level != 0 {
                    return Err(Error::Group);
                }
                if !self.conditions.is_empty() {
                    return Err(Error::Conditional);
                }
                return Ok(Outcome {
                    counts: self.scalars.counts().map(|i| i.value),
                    messages: self.messages,
                });
            }
            self.execute(read)?;
        }
    }
    pub(crate) fn intern(&mut self, name: &[u8]) -> Result<u32> {
        let id = self.names.intern(name)?;
        if id as usize >= self.bindings.len() {
            self.bindings.resize(id as usize + 1, Binding::default());
        }
        Ok(id)
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
            // TeX distinguishes this one-read relax from the ordinary primitive
            // when a raw scanner (notably ifx) inspects it.
            meaning = Meaning {
                command: cmd::RELAX,
                value: 257,
            };
        }
        Ok(Read {
            token: raw.token,
            meaning,
        })
    }
    #[inline]
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
mod tests;
