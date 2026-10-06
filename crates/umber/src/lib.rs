mod execute;
mod expand;
mod input;
mod macros;
mod scan;
mod token;
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
    pub(crate) counts: [Integer; 256],
    pub(crate) catcodes: [Integer; 256],
    pub(crate) endline: Integer,
    pub(crate) arena: TokenArena,
    pub(crate) definitions: Vec<Definition>,
    pub(crate) sources: Vec<Source>,
    pub(crate) captures: Vec<Capture>,
    pub(crate) packets: Vec<Packet>,
    pub(crate) free_packets: Vec<u32>,
    pub(crate) save: Vec<execute::Save>,
    pub(crate) level: u16,
    pub(crate) conditions: Vec<expand::Condition>,
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
            counts: [Integer::default(); 256],
            catcodes: [Integer {
                value: 12,
                level: 0,
            }; 256],
            endline: Integer {
                value: 13,
                level: 0,
            },
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
        for b in b'a'..=b'z' {
            engine.catcodes[b as usize].value = 11;
        }
        for b in b'A'..=b'Z' {
            engine.catcodes[b as usize].value = 11;
        }
        for (b, cat) in [
            (b'\\', 0),
            (b' ', 10),
            (13, 5),
            (b'%', 14),
            (0, 9),
            (127, 15),
        ] {
            engine.catcodes[b as usize].value = cat;
        }
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
        ];
        engine.bindings[0].meaning = Meaning::RELAX;
        for &(name, command, value) in primitives {
            let id = engine.intern(name);
            engine.bindings[id as usize].meaning = Meaning { command, value };
            if command == cmd::PAR {
                engine.par = Token::cs(id);
            }
        }
        engine
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
                    counts: self.counts.map(|i| i.value),
                    messages: self.messages,
                });
            }
            self.execute(read)?;
        }
    }
    pub(crate) fn intern(&mut self, name: &[u8]) -> u32 {
        let id = self.names.intern(name);
        if id as usize >= self.bindings.len() {
            self.bindings.resize(id as usize + 1, Binding::default());
        }
        id
    }
    #[inline(always)]
    pub(crate) fn resolve(&self, token: Token) -> Meaning {
        if token.is_cs() {
            self.bindings[token.id()].meaning
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
        Ok(Read { raw, meaning })
    }
    #[inline]
    pub(crate) fn next_expanded(&mut self) -> Result<Read> {
        loop {
            let read = self.next_token()?;
            if !read.meaning.expandable() {
                return Ok(read);
            }
            if read.meaning.command == cmd::MACRO {
                self.begin_macro(read.meaning.value as u32)?;
            } else {
                self.expand(read)?;
            }
        }
    }
}
#[cfg(test)]
mod tests;
