//! External modes and owned horizontal construction; paragraph completion is a hook.
use crate::{
    Engine, Error, Result,
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
    Horizontal,
}
pub(crate) struct TextState {
    pub mode: Mode,
    pub horizontal: List,
    pub vertical: List,
    pub space_factor: i32,
    pub prev_depth: Scaled,
    pub prev_graf: i32,
    pub cancel_boundary: bool,
}
impl Default for TextState {
    fn default() -> Self {
        Self {
            mode: Mode::Vertical,
            horizontal: List::default(),
            vertical: List::default(),
            space_factor: 1000,
            prev_depth: Scaled(-65_536_000),
            prev_graf: 0,
            cancel_boundary: false,
        }
    }
}
impl TextState {
    pub fn snapshot(&self) -> Self {
        Self {
            mode: self.mode,
            horizontal: self.horizontal.snapshot(),
            vertical: self.vertical.snapshot(),
            space_factor: self.space_factor,
            prev_depth: self.prev_depth,
            prev_graf: self.prev_graf,
            cancel_boundary: self.cancel_boundary,
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
        let spec = self.document.layout_values.glues[GlueParam::Paragraph.index()].value;
        let node = self.document.nodes.alloc(Kind::Glue {
            spec,
            subtype: GlueKind::ParSkip,
        })?;
        self.document
            .nodes
            .append(&mut self.document.text.vertical, node);
        self.document.text.mode = Mode::Horizontal;
        self.document.text.space_factor = 1000;
        if indented {
            self.indent()?;
        }
        if let Some(owner) = self.document.token_registers[256].owner.clone() {
            self.push_tokens(owner, 0)?;
        }
        Ok(())
    }
    fn indent(&mut self) -> Result<()> {
        let b = BoxData {
            width: self.document.layout_values.dimens[DimenParam::ParIndent.index()].value,
            ..BoxData::default()
        };
        self.append_horizontal(Kind::Box(b))?;
        self.document.text.space_factor = 1000;
        Ok(())
    }
    pub(crate) fn end_graf(&mut self) -> Result<()> {
        if self.document.text.mode == Mode::Horizontal {
            if !self.document.text.horizontal.is_empty() {
                return Err(Error::Unsupported);
            }
            self.document.text.mode = Mode::Vertical;
        }
        Ok(())
    }
    /// Returns true when this complete command belongs to horizontal execution.
    pub(crate) fn text_command(&mut self, read: Read) -> Result<bool> {
        let cmd = read.meaning.command;
        if self.document.text.mode == Mode::Vertical
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
            11 | 12 | cmd::CHAR_GIVEN | cmd::CHAR => {
                if self.document.text.mode == Mode::Vertical {
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
                if self.document.text.mode == Mode::Horizontal {
                    self.append_space(false)?;
                }
            }
            cmd::CONTROL_SPACE => {
                if self.document.text.mode == Mode::Vertical {
                    self.new_graf(true)?;
                }
                self.append_space(true)?;
            }
            cmd::PAR => self.end_graf()?,
            cmd::START_PAR => {
                if self.document.text.mode == Mode::Vertical {
                    self.new_graf(read.meaning.value != 0)?;
                } else if read.meaning.value != 0 {
                    self.indent()?;
                }
            }
            cmd::NO_BOUNDARY => {
                if self.document.text.mode == Mode::Vertical {
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
                if self.document.text.mode == Mode::Vertical {
                    self.new_graf(true)?;
                }
                let spec = self.scan_glue()?;
                self.append_horizontal(Kind::Glue {
                    spec,
                    subtype: GlueKind::Normal,
                })?;
            }
            cmd::KERN => {
                let width = self.scan_dimen()?;
                let node = self.document.nodes.alloc(Kind::Kern {
                    width,
                    explicit: true,
                })?;
                if self.document.text.mode == Mode::Horizontal {
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
    pub(crate) fn back_read(&mut self, read: Read) {
        self.push_inline(
            [read.token, Token(0)],
            1,
            read.meaning == Meaning::NOEXPAND_RELAX,
        );
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
