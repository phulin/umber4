//! Owned external/internal text lists and paragraph handoff.
use crate::{
    Engine, Error, Result,
    language::Language,
    layout_values::{DimenParam, GlueParam},
    nodes::{BoxData, GlueKind, Kind, List},
    numeric::{Glue, Scaled},
    scalars::Target,
    tfm::ratio,
    token::*,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    #[default]
    Vertical,
    InternalVertical,
    Horizontal,
    RestrictedHorizontal,
    Write,
}
impl Mode {
    pub fn is_horizontal(self) -> bool {
        matches!(self, Self::Horizontal | Self::RestrictedHorizontal)
    }
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Vertical | Self::InternalVertical)
    }
    pub fn is_inner(self) -> bool {
        matches!(self, Self::RestrictedHorizontal | Self::InternalVertical)
    }
}
pub(crate) struct TextState {
    pub mode: Mode,
    pub vertical_mode: Mode,
    pub horizontal: List,
    pub vertical: List,
    pub space_factor: i32,
    pub prev_depth: Scaled,
    pub prev_graf: i32,
    pub cancel_boundary: bool,
    pub initial_language: Language,
    pub language: u8,
}
impl Default for TextState {
    fn default() -> Self {
        Self {
            mode: Mode::Vertical,
            vertical_mode: Mode::Vertical,
            horizontal: List::default(),
            vertical: List::default(),
            space_factor: 1000,
            prev_depth: Scaled(-65_536_000),
            prev_graf: 0,
            cancel_boundary: false,
            initial_language: Language::default(),
            language: 0,
        }
    }
}
impl TextState {
    pub fn snapshot(&self) -> Self {
        Self {
            mode: self.mode,
            vertical_mode: self.vertical_mode,
            horizontal: self.horizontal.snapshot(),
            vertical: self.vertical.snapshot(),
            space_factor: self.space_factor,
            prev_depth: self.prev_depth,
            prev_graf: self.prev_graf,
            cancel_boundary: self.cancel_boundary,
            initial_language: self.initial_language,
            language: self.language,
        }
    }
}
impl Engine<'_> {
    pub(crate) fn append_horizontal(&mut self, kind: Kind) -> Result<()> {
        let node = self.document.nodes.alloc(kind)?;
        self.document
            .nodes
            .append(&mut self.document.text.horizontal, node);
        Ok(())
    }
    pub(crate) fn new_graf(&mut self, indented: bool) -> Result<()> {
        self.document.text.prev_graf = 0;
        self.document.text.vertical_mode = self.document.text.mode;
        if self.document.text.mode == Mode::Vertical || !self.document.text.vertical.is_empty() {
            let spec = self.document.layout_values.glues[GlueParam::Paragraph.index()].value;
            let node = self.document.nodes.alloc(Kind::Glue {
                spec,
                subtype: GlueKind::ParSkip,
            })?;
            self.document
                .nodes
                .append(&mut self.document.text.vertical, node);
        }
        self.document.text.mode = Mode::Horizontal;
        self.document.text.space_factor = 1000;
        let initial = Language::capture(
            &self.document.scalars,
            self.document.scalars[Target::LANGUAGE].value,
        );
        self.document.text.initial_language = initial;
        self.document.text.language = initial.number;
        if indented {
            self.indent()?;
        }
        if let Some(owner) = self.document.token_registers[256].owner.clone() {
            self.push_tokens(owner, 0)?;
        }
        if self.document.text.vertical_mode == Mode::Vertical {
            self.build_page()?;
        }
        Ok(())
    }
    fn indent(&mut self) -> Result<()> {
        let b = BoxData {
            width: self.document.layout_values.dimens[DimenParam::ParIndent.index()].value,
            ..BoxData::default()
        };
        let node = self.document.nodes.alloc_box(b)?;
        self.document
            .nodes
            .append(&mut self.document.text.horizontal, node);
        self.document.text.space_factor = 1000;
        Ok(())
    }
    pub(crate) fn end_graf(&mut self) -> Result<()> {
        if self.document.text.mode == Mode::Horizontal {
            if !self.document.text.horizontal.is_empty() {
                self.break_paragraph()?;
            }
            self.document.text.mode = self.document.text.vertical_mode;
            self.normal_paragraph()?;
            if self.document.text.mode == Mode::Vertical {
                self.build_page()?;
            }
        }
        Ok(())
    }
    /// Returns true when this complete command belongs to horizontal execution.
    pub(crate) fn text_command(&mut self, read: Read) -> Result<bool> {
        if read.meaning.command == cmd::MAKE_BOX {
            self.make_box(
                read.meaning.value,
                crate::boxes::Destination::Append(Scaled::ZERO),
            )?;
            return Ok(true);
        }
        if read.meaning.command == cmd::BOX_SHIFT {
            self.shift_box(read.meaning.value)?;
            return Ok(true);
        }
        if read.meaning.command == cmd::UNBOX {
            if read.meaning.value < 2 && self.document.text.mode.is_vertical() {
                self.back_read(read);
                self.new_graf(true)?;
            } else if read.meaning.value >= 2 && self.document.text.mode == Mode::Horizontal {
                self.head_for_vertical(read);
            } else {
                self.unbox(read.meaning.value)?;
            }
            return Ok(true);
        }
        if self.vertical_command(read)? {
            return Ok(true);
        }
        let cmd = read.meaning.command;
        if self.document.text.mode.is_vertical()
            && matches!(
                cmd,
                11 | 12
                    | cmd::CHAR_GIVEN
                    | cmd::CHAR
                    | cmd::CONTROL_SPACE
                    | cmd::NO_BOUNDARY
                    | cmd::HSKIP
            )
        {
            self.back_read(read);
            self.new_graf(true)?;
            return Ok(true);
        }
        match cmd {
            cmd::ITALIC_CORRECTION => self.append_italic_correction()?,
            11 | 12 | cmd::CHAR_GIVEN | cmd::CHAR => {
                if self.document.text.mode.is_vertical() {
                    self.new_graf(true)?;
                }
                let c = if cmd == cmd::CHAR {
                    self.scan_index()? as u8
                } else {
                    read.meaning.value as u8
                };
                self.character_run(c)?;
            }
            cmd::SPACE => {
                if self.document.text.mode.is_horizontal() {
                    self.append_space(false)?;
                }
            }
            cmd::CONTROL_SPACE => {
                if self.document.text.mode.is_vertical() {
                    self.new_graf(true)?;
                }
                self.append_space(true)?;
            }
            cmd::PAR => {
                if self.document.text.mode.is_vertical() {
                    self.normal_paragraph()?;
                    if self.document.text.mode == Mode::Vertical {
                        self.build_page()?;
                    }
                } else {
                    self.end_graf()?;
                }
            }
            cmd::START_PAR => {
                if self.document.text.mode.is_vertical() {
                    self.new_graf(read.meaning.value != 0)?;
                } else if read.meaning.value != 0 {
                    self.indent()?;
                }
            }
            cmd::NO_BOUNDARY => {
                if self.document.text.mode.is_vertical() {
                    self.new_graf(true)?;
                }
                // TeX consumes one expanded lookahead; cancellation applies only
                // when that immediate command is a character, not an assignment.
                let next = self.next_expanded()?;
                self.document.text.cancel_boundary =
                    matches!(next.meaning.command, 11 | 12 | cmd::CHAR_GIVEN | cmd::CHAR);
                self.back_read(next);
            }
            cmd::HSKIP => {
                if self.document.text.mode.is_vertical() {
                    self.new_graf(true)?;
                }
                let spec = self.scan_skip(read.meaning.value)?;
                self.append_horizontal(Kind::Glue {
                    spec,
                    subtype: GlueKind::Normal,
                })?;
            }
            cmd::PENALTY => {
                let penalty = self.scan_int()?;
                let node = self.document.nodes.alloc(Kind::Penalty(penalty))?;
                let list = if self.document.text.mode.is_horizontal() {
                    &mut self.document.text.horizontal
                } else {
                    &mut self.document.text.vertical
                };
                self.document.nodes.append(list, node);
                if self.document.text.mode == Mode::Vertical {
                    self.build_page()?;
                }
            }
            cmd::KERN => {
                let width = self.scan_dimen()?;
                let node = self.document.nodes.alloc(Kind::Kern {
                    width,
                    explicit: true,
                })?;
                if self.document.text.mode.is_horizontal() {
                    self.document
                        .nodes
                        .append(&mut self.document.text.horizontal, node);
                } else {
                    self.document
                        .nodes
                        .append(&mut self.document.text.vertical, node);
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
    pub(crate) fn head_for_vertical(&mut self, read: Read) {
        self.back_read(read);
        self.back(self.document.par);
    }
    fn append_italic_correction(&mut self) -> Result<()> {
        if !self.document.text.mode.is_horizontal() {
            return Err(Error::Syntax);
        }
        let tail = self.document.text.horizontal.tail();
        if tail.is_null() {
            return Ok(());
        }
        let (Kind::Character { font, character }
        | Kind::Ligature {
            font, character, ..
        }) = self.document.nodes.get(tail).kind
        else {
            return Ok(());
        };
        let width = self
            .character_metrics(font, character)
            .ok_or(Error::Font)?
            .italic;
        self.append_horizontal(Kind::Kern {
            width,
            explicit: true,
        })
    }
    pub(crate) fn back_read(&mut self, read: Read) {
        self.push_backed(read.token, read.meaning == Meaning::NOEXPAND_RELAX);
    }
    pub(crate) fn adjust_space_factor(&mut self, c: u8) {
        let n = self.document.scalars[Target::sfcode(c as usize)].value;
        let sf = &mut self.document.text.space_factor;
        if n == 1000 {
            *sf = 1000;
        } else if n < 1000 {
            if n > 0 {
                *sf = n;
            }
        } else if *sf < 1000 {
            *sf = 1000;
        } else {
            *sf = n;
        }
    }
    fn append_space(&mut self, normal: bool) -> Result<()> {
        let font = self.document.fonts.current.value as usize;
        let sf = self.document.text.space_factor;
        let spaces = self.document.layout_values.glues[GlueParam::Space.index()].value;
        let xspaces = self.document.layout_values.glues[GlueParam::XSpace.index()].value;
        let mut g = if spaces != Glue::default() {
            spaces
        } else {
            Glue {
                width: self.font_parameter(font, 2),
                stretch: self.font_parameter(font, 3),
                shrink: self.font_parameter(font, 4),
                ..Glue::default()
            }
        };
        let mut subtype = if spaces != Glue::default() {
            GlueKind::SpaceSkip
        } else {
            GlueKind::Normal
        };
        if !normal && sf != 1000 {
            subtype = GlueKind::Normal;
            if sf >= 2000 && xspaces != Glue::default() {
                g = xspaces;
                subtype = GlueKind::XSpaceSkip;
            } else {
                if sf >= 2000 {
                    g.width = g.width.advance(self.font_parameter(font, 7));
                }
                g.stretch = Scaled(ratio(g.stretch.0, sf, 1000)?);
                g.shrink = Scaled(ratio(g.shrink.0, 1000, sf)?);
            }
        }
        self.append_horizontal(Kind::Glue { spec: g, subtype })
    }
}
