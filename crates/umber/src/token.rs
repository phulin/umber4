pub(crate) const CS: u32 = 1 << 24;
pub(crate) const PARAM: u32 = 1 << 23;
pub(crate) const SINGLE: u32 = u32::MAX - 1;
pub(crate) const FROZEN_RELAX: Token = Token(CS);
pub(crate) const CHUNK_BITS: u32 = 12;
// Control-sequence IDs follow TeX's layout: the frozen relax, the empty name,
// one single-character name and one active character per byte, then interned
// multi-byte names.
pub(crate) const NULL_CS: u32 = 1;
pub(crate) const SINGLE_BASE: u32 = 2;
pub(crate) const ACTIVE_BASE: u32 = SINGLE_BASE + 256;
pub(crate) const NAMED_BASE: u32 = ACTIVE_BASE + 256;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Token(pub u32);
impl Token {
    pub const SPACE: Self = Self((10 << 8) | 32);
    pub fn character(cat: u8, byte: u8) -> Self {
        Self((u32::from(cat) << 8) | u32::from(byte))
    }
    pub fn cs(id: u32) -> Self {
        Self(CS | id)
    }
    pub fn single(byte: u8) -> Self {
        Self::cs(SINGLE_BASE + u32::from(byte))
    }
    pub fn active(byte: u8) -> Self {
        Self::cs(ACTIVE_BASE + u32::from(byte))
    }
    pub fn single_byte(self) -> Option<u8> {
        let offset = self.0.wrapping_sub(CS + SINGLE_BASE);
        (offset < 256).then_some(offset as u8)
    }
    pub fn active_byte(self) -> Option<u8> {
        let offset = self.0.wrapping_sub(CS + ACTIVE_BASE);
        (offset < 256).then_some(offset as u8)
    }
    pub fn is_cs(self) -> bool {
        self.0 >= CS
    }
    pub fn id(self) -> usize {
        (self.0 - CS) as usize
    }
    pub fn cat(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub fn byte(self) -> u8 {
        self.0 as u8
    }
    pub fn is_begin(self) -> bool {
        self.0 >> 8 == 1
    }
    pub fn is_end(self) -> bool {
        self.0 >> 8 == 2
    }
    pub fn is_space(self) -> bool {
        self == Self::SPACE
    }
    /// A parameter marker in a stored definition or argument.
    pub fn is_parameter(self) -> bool {
        self.0 & PARAM != 0 && !self.is_cs()
    }
    /// A character that cannot change group depth: neither a brace, a
    /// control sequence, nor a parameter marker.
    pub fn is_plain_character(self) -> bool {
        (3 << 8..PARAM).contains(&self.0)
    }
    pub fn other(byte: u8) -> Self {
        Self::character(12, byte)
    }
    /// Converted text: spaces become blank spaces, other bytes other characters.
    pub fn string_character(byte: u8) -> Self {
        if byte == b' ' {
            Self::SPACE
        } else {
            Self::other(byte)
        }
    }
}
pub(crate) mod cmd {
    // One inventory supplies command codes, registration, text, and assignment
    // classification. Names within a command are indexed by its meaning value.
    macro_rules! commands {
        ($($command:ident = $code:literal => [$($name:literal),*] $($kind:ident)?;)*) => {
            $(pub const $command: u8 = $code;)*
            pub const PRIMITIVES: &[(u8, &[&[u8]])] = &[
                $(($command, &[$($name),*]),)*
            ];
            pub fn names(command: u8) -> &'static [&'static [u8]] {
                match command {
                    $($command => &[$($name),*],)*
                    _ => &[],
                }
            }
            pub fn is_assignment(command: u8) -> bool {
                match command {
                    $($command => commands!(@assignment $($kind)?),)*
                    _ => false,
                }
            }
            pub fn is_conditional(command: u8) -> bool {
                match command {
                    $($command => commands!(@conditional $($kind)?),)*
                    _ => false,
                }
            }
        };
        (@assignment assignment) => { true };
        (@assignment conditional) => { false };
        (@assignment) => { false };
        (@conditional conditional) => { true };
        (@conditional assignment) => { false };
        (@conditional) => { false };
    }
    commands! {
        COUNT_REF = 32 => [] assignment;
        TOKS_REF = 37 => [] assignment;
        BEGIN = 1 => [];
        END_GROUP_CHAR = 2 => [];
        SPACE = 10 => [];
        CHAR_GIVEN = 38 => [];
        MACRO = 64 => [];
        PROTECTED_MACRO = 65 => [];
        UNDEFINED = 66 => [];
        RELAX = 16 => [b"relax"];
        DEF = 17 => [b"def"] assignment;
        GDEF = 18 => [b"gdef"] assignment;
        EDEF = 19 => [b"edef"] assignment;
        XDEF = 20 => [b"xdef"] assignment;
        LET = 21 => [b"let"] assignment;
        GLOBAL = 22 => [b"global"];
        LONG = 23 => [b"long"];
        BEGIN_GROUP = 24 => [b"begingroup"];
        END_GROUP = 25 => [b"endgroup"];
        COUNT = 26 => [b"count"] assignment;
        COUNT_DEF = 27 => [b"countdef"] assignment;
        ARITH = 28 => [b"advance", b"multiply", b"divide"] assignment;
        CATCODE = 29 => [b"catcode"] assignment;
        ENDLINE = 30 => [b"endlinechar"] assignment;
        MESSAGE = 31 => [b"message"];
        NUMBER = 67 => [b"number"];
        THE = 68 => [b"the"];
        EXPANDAFTER = 69 => [b"expandafter"];
        NOEXPAND = 70 => [b"noexpand"];
        CSNAME = 71 => [b"csname"];
        ENDCSNAME = 33 => [b"endcsname"];
        IFNUM = 72 => [b"ifnum"] conditional;
        IFX = 73 => [b"ifx"] conditional;
        IFTRUE = 74 => [b"iftrue"] conditional;
        IFFALSE = 75 => [b"iffalse"] conditional;
        ELSE = 76 => [b"else"];
        FI = 77 => [b"fi"];
        END = 34 => [b"end"];
        PAR = 35 => [b"par"];
        TOKS = 36 => [b"toks"] assignment;
        TOKS_DEF = 41 => [b"toksdef"] assignment;
        CHAR_DEF = 39 => [b"chardef"] assignment;
        FUTURELET = 40 => [b"futurelet"] assignment;
        AFTER_ASSIGN = 42 => [b"afterassignment"];
        AFTER_GROUP = 43 => [b"aftergroup"];
        INTEGER_PARAM = 44 => [b"globaldefs", b"escapechar", b"time", b"day", b"month", b"year"] assignment;
        CASE_CODE = 45 => [b"lccode", b"uccode"] assignment;
        CHANGE_CASE = 46 => [b"lowercase", b"uppercase"];
        PROTECTED = 47 => [b"protected"];
        NUMEXPR = 48 => [b"numexpr"];
        STRING = 78 => [b"string"];
        MEANING = 80 => [b"meaning"];
        ROMAN = 79 => [b"romannumeral"];
        DETOKENIZE = 89 => [b"detokenize"];
        UNEXPANDED = 88 => [b"unexpanded"];
        IF = 81 => [b"if"] conditional;
        IFCAT = 82 => [b"ifcat"] conditional;
        IFODD = 83 => [b"ifodd"] conditional;
        IFCASE = 84 => [b"ifcase"] conditional;
        OR = 85 => [b"or"];
        IFDEFINED = 86 => [b"ifdefined"] conditional;
        IFCSNAME = 87 => [b"ifcsname"] conditional;
        UNLESS = 90 => [b"unless"];
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Meaning {
    pub command: u8,
    pub value: i32,
}
impl Meaning {
    pub const UNDEFINED: Self = Self {
        command: cmd::UNDEFINED,
        value: 0,
    };
    pub const RELAX: Self = Self {
        command: cmd::RELAX,
        value: 0,
    };
    // TeX distinguishes the one-read relax of a noexpanded token from the
    // ordinary primitive when a raw scanner (notably ifx) inspects it.
    pub const NOEXPAND_RELAX: Self = Self {
        command: cmd::RELAX,
        value: 257,
    };
    pub fn is_macro(self) -> bool {
        matches!(self.command, cmd::MACRO | cmd::PROTECTED_MACRO)
    }
    pub fn is_conditional(self) -> bool {
        cmd::is_conditional(self.command)
    }
    pub fn expandable(self) -> bool {
        self.command >= cmd::MACRO
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Binding {
    pub value: i32,
    pub level: u16,
    pub command: u8,
}
impl Binding {
    pub fn new(meaning: Meaning, level: u16) -> Self {
        Self {
            value: meaning.value,
            command: meaning.command,
            level,
        }
    }
    pub fn meaning(self) -> Meaning {
        Meaning {
            command: self.command,
            value: self.value,
        }
    }
}
impl Default for Binding {
    fn default() -> Self {
        Self::new(Meaning::UNDEFINED, 0)
    }
}
#[derive(Clone, Copy, Default)]
pub(crate) struct Integer {
    pub value: i32,
    pub level: u16,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ArenaRange {
    pub start: u32,
    pub end: u32,
}
impl ArenaRange {
    pub fn empty(self) -> bool {
        self.start == self.end
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Span {
    pub store: u32,
    pub start: u32,
    pub end: u32,
}
impl Span {
    pub fn empty(self) -> bool {
        self.start == self.end
    }
    // SINGLE encodes an inline token in start with end = start + 1. It is
    // confined to a one-token sequence and promoted before another range joins.
    pub fn single(token: Token) -> Self {
        Self {
            store: SINGLE,
            start: token.0,
            end: token.0 + 1,
        }
    }
    // A delimiter trim can empty an inline word; it then holds no token.
    pub fn single_token(self) -> Option<Token> {
        if self.store == SINGLE && !self.empty() {
            Some(Token(self.start))
        } else {
            None
        }
    }
}
#[derive(Default)]
pub(crate) struct Sequence {
    pub first: Span,
    pub rest: Vec<Span>,
}
impl Sequence {
    #[inline(always)]
    pub fn append(&mut self, span: Span) -> bool {
        if span.empty() {
            return false;
        }
        let last = self.rest.last_mut().unwrap_or(&mut self.first);
        if last.empty() {
            *last = span;
        } else if last.store == span.store && last.end == span.start {
            last.end = span.end;
            return false;
        } else {
            self.rest.push(span);
        }
        true
    }
    pub fn span(&self, index: usize) -> Option<Span> {
        if index == 0 {
            (!self.first.empty()).then_some(self.first)
        } else {
            self.rest.get(index - 1).copied()
        }
    }
    pub fn trim_front(&mut self) {
        self.first.start += 1;
        if self.first.empty() && !self.rest.is_empty() {
            self.first = self.rest.remove(0);
        }
    }
    pub fn trim_back(&mut self, mut count: usize) {
        while count > 0 {
            let last = self.rest.last_mut().unwrap_or(&mut self.first);
            let n = count.min((last.end - last.start) as usize);
            last.end -= n as u32;
            count -= n;
            if last.empty() && !self.rest.is_empty() {
                self.rest.pop();
            }
        }
    }
    // Ordinary builds keep the packet loop small for common one-argument
    // packets. The trained compiler chooses its own reset-loop inlining.
    #[cfg_attr(not(feature = "native-profile"), inline(never))]
    pub fn clear(&mut self) {
        self.first = Span::default();
        self.rest.clear();
    }
}
#[derive(Default)]
pub(crate) struct TokenValue {
    pub owner: Option<crate::values::Owner>,
    pub level: u16,
}
// Keep invocation fields ahead of cold fallback storage. The default Rust
// layout put body/prefix fields beyond another cache line after adding vectors.
#[repr(C)]
#[derive(Default)]
pub(crate) struct Definition {
    // The owning value contains the full prefix, parameter markers, separator,
    // and replacement text. body selects the executable part of that value.
    pub prefix_end: u32,
    pub body: ArenaRange,
    pub long: bool,
    pub uses_parameters: bool,
    pub parameters: crate::value_metadata::Buffer<Parameter>,
    pub failure: crate::value_metadata::Buffer<usize>,
}
impl Definition {
    pub fn clear(&mut self) {
        self.parameters.clear();
        self.failure.clear();
    }
    #[cfg(feature = "native-profile")]
    pub fn failure(&self, argument: usize) -> &[usize] {
        let parameter = &self.parameters[argument];
        let length = (parameter.delimiter.end - parameter.delimiter.start) as usize;
        &self.failure[parameter.failure_start..parameter.failure_start + length]
    }
}
pub(crate) struct Parameter {
    pub delimiter: ArenaRange,
    pub failure_start: usize,
}
#[derive(Default)]
pub(crate) struct Packet {
    pub arguments: Vec<Sequence>,
    pub owners: Vec<crate::values::Owner>,
}
impl Packet {
    pub fn clear(&mut self) {
        for argument in &mut self.arguments {
            argument.clear();
        }
        self.owners.clear();
    }
}
pub(crate) struct Cursor {
    pub next: *const Token,
    pub end: *const Token,
    pub end_position: u32,
    pub store: u32,
    pub _owner: Option<crate::values::Owner>,
    pub argument: Option<(u32, u8)>,
    pub span_index: usize,
    pub span_end: u32,
    pub owns_packet: Option<u32>,
}
pub(crate) enum Source {
    Tokens(Cursor),
    Inline {
        tokens: [Token; 2],
        position: u8,
        len: u8,
        suppress: bool,
    },
}
impl Source {
    #[inline(always)]
    pub fn cursor(&self) -> Option<&Cursor> {
        match self {
            Self::Tokens(cursor) => Some(cursor),
            Self::Inline { .. } => None,
        }
    }
    #[inline(always)]
    pub fn cursor_mut(&mut self) -> Option<&mut Cursor> {
        match self {
            Self::Tokens(cursor) => Some(cursor),
            Self::Inline { .. } => None,
        }
    }
}
#[derive(Clone, Copy)]
pub(crate) struct Raw {
    pub token: Token,
    pub origin: Option<Span>,
    pub suppress: bool,
}
#[derive(Clone, Copy)]
pub(crate) struct Read {
    pub token: Token,
    pub meaning: Meaning,
}
