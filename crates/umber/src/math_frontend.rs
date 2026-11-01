//! Inline semantic owners remain distinct from ordinary box construction.
use crate::nodes::KernKind;
use crate::{
    Engine, Error, Result,
    boxes::Destination,
    execute::GroupKind,
    layout_values::DimenParam,
    math::{
        Atom, Class, Delimiter, Field, FieldSlot, FieldTarget, Fields, Kind, Limits, MathChar,
        MathId, MathList, RuleThickness, Side, Style,
    },
    nodes::{self, DetachedNode},
    numeric::Scaled,
    scalars::Target,
    text::{Mode, TextState},
    token::*,
};

pub(crate) enum Context {
    Inline(TextState),
    Field(FieldTarget),
    LeftRight,
    Choice {
        atom: MathId,
        branch: u8,
    },
    Display {
        number: Option<DetachedNode>,
        left: bool,
    },
    EquationNumber {
        left: bool,
    },
}
pub(crate) struct Frame {
    pub list: MathList,
    pub incomplete: Option<MathList>,
    pub context: Context,
    pub level: u16,
    pub enclosing_mode: Mode,
}
impl Engine<'_> {
    pub(crate) fn append_math(&mut self, kind: Kind) -> Result<MathId> {
        let frame = self
            .document
            .math_frames
            .last_mut()
            .expect("active math owner");
        self.math
            .as_mut()
            .expect("math store outside conversion")
            .append(&mut frame.list, kind, &mut self.document.nodes)
    }
    pub(crate) fn append_math_box(&mut self, node: DetachedNode) -> Result<()> {
        self.append_math(Kind::Atom(Atom::new(Class::Ord, Field::Box(node))))?;
        Ok(())
    }
    pub(crate) fn append_math_vcenter(&mut self, node: DetachedNode) -> Result<()> {
        self.append_math(Kind::VCenter(Fields::new(Field::Box(node))))?;
        Ok(())
    }
    fn math_material(&mut self, kind: nodes::Kind) -> Result<()> {
        let node = self.document.nodes.alloc(kind)?;
        self.append_math(Kind::Material(node))?;
        Ok(())
    }
    pub(crate) fn math_command(&mut self, read: Read) -> Result<bool> {
        let command = read.meaning.command;
        if command == 3 {
            match self.document.text.mode {
                Mode::Vertical | Mode::InternalVertical => {
                    self.back_read(read);
                    self.new_graf(true)?;
                }
                Mode::Horizontal | Mode::RestrictedHorizontal => self.enter_inline_math()?,
                Mode::Math | Mode::DisplayMath => self.finish_math_shift()?,
                Mode::Write => return Err(Error::Syntax),
            }
            return Ok(true);
        }
        if !self.document.text.mode.is_math() {
            return Ok(false);
        }
        match command {
            11
            | 12
            | cmd::CHAR_GIVEN
            | cmd::CHAR
            | cmd::MATH_GIVEN
            | cmd::MATH_CHAR
            | cmd::DELIMITER => {
                if let Some(code) = self.math_character_code(read)? {
                    let class = if code >= 0x7000 {
                        Class::Ord
                    } else {
                        class(code >> 12)
                    };
                    let field = self.math_character_field(code);
                    self.append_math(Kind::Atom(Atom::new(class, field)))?;
                }
            }
            cmd::BEGIN => {
                let atom = self.append_math(Kind::Atom(Atom::new(Class::Ord, Field::Empty)))?;
                self.back_read(read);
                self.scan_math_field(FieldTarget {
                    atom,
                    slot: FieldSlot::Nucleus,
                })?;
            }
            7 | 8 => self.math_script(command == 7)?,
            cmd::MATH_CLASS => {
                let atom = self.append_math(Kind::Atom(Atom::new(
                    class(read.meaning.value),
                    Field::Empty,
                )))?;
                self.scan_math_field(FieldTarget {
                    atom,
                    slot: FieldSlot::Nucleus,
                })?;
            }
            cmd::MATH_STYLE => {
                self.append_math(Kind::Style(
                    Style::new(read.meaning.value as u8 * 2).unwrap(),
                ))?;
            }
            cmd::LEFT_RIGHT => self.math_left_right(read.meaning.value != 0)?,
            cmd::MATH_FRACTION => self.math_fraction(read.meaning.value)?,
            cmd::MATH_FORM => self.math_form(read.meaning.value)?,
            cmd::VCENTER => self.make_vcenter()?,
            cmd::MATH_CHOICE => self.math_choice()?,
            cmd::EQNO => self.begin_equation_number(read.meaning.value != 0)?,
            cmd::MATH_LIMITS => {
                let tail = self
                    .document
                    .math_frames
                    .last()
                    .unwrap()
                    .list
                    .tail()
                    .ok_or(Error::Syntax)?;
                let limits = match read.meaning.value {
                    0 => Limits::Normal,
                    1 => Limits::AboveBelow,
                    _ => Limits::Side,
                };
                self.math.as_mut().unwrap().set_limits(tail, limits)?;
            }
            cmd::SPACE | cmd::NO_BOUNDARY => {}
            cmd::CONTROL_SPACE => self.math_material(self.space_node(true)?)?,
            cmd::KERN => {
                let width = self.scan_dimen()?;
                self.math_material(nodes::Kind::Kern {
                    width,
                    kind: KernKind::Explicit,
                })?;
            }
            cmd::LEADERS => self.begin_leaders(read.meaning.value)?,
            cmd::INSERT_ADJUST => self.begin_insertion(read.meaning.value != 0)?,
            cmd::MKERN => {
                let width = self.scan_mu_dimen()?;
                self.append_math(Kind::MuKern(width))?;
            }
            cmd::ITALIC_CORRECTION => self.math_material(nodes::Kind::Kern {
                width: Scaled::ZERO,
                kind: KernKind::Normal,
            })?,
            cmd::HSKIP => {
                let spec = self.scan_skip(read.meaning.value)?;
                self.math_material(nodes::Kind::Glue {
                    spec,
                    subtype: nodes::GlueKind::Normal,
                })?;
            }
            cmd::PENALTY => {
                let penalty = self.scan_int()?;
                self.math_material(nodes::Kind::Penalty(penalty))?;
            }
            cmd::RULE if read.meaning.value == 1 => {
                let rule = self.scan_rule(true)?;
                self.math_material(rule)?;
            }
            cmd::MAKE_BOX => {
                self.make_box(read.meaning.value, Destination::MathAppend(Scaled::ZERO))?;
            }
            cmd::BOX_SHIFT => self.shift_box(read.meaning.value)?,
            cmd::PAR | cmd::VSKIP | cmd::RULE | cmd::ACCENT => return Err(Error::Syntax),
            cmd::UNBOX | cmd::START_PAR => return Err(Error::Unsupported),
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn enter_inline_math(&mut self) -> Result<()> {
        // Native init_math uses get_token, before the mode change. Expansion
        // here would evaluate a following ifmmode in the enclosing text mode.
        let lookahead = self.next_token()?;
        if lookahead.meaning.command == 3 && self.document.text.mode == Mode::Horizontal {
            return self.enter_display_math();
        }
        self.back(lookahead.token);
        self.open_group(GroupKind::MathShift)?;
        let outer = std::mem::replace(
            &mut self.document.text,
            TextState {
                mode: Mode::Math,
                vertical_mode: Mode::Math,
                ..TextState::default()
            },
        );
        let outer_mode = outer.mode;
        self.document.math_frames.push(Frame {
            list: MathList::default(),
            incomplete: None,
            context: Context::Inline(outer),
            level: self.document.level,
            enclosing_mode: outer_mode,
        });
        self.update_integer(Target::FAM, false, |_| Ok(-1))?;
        if let Some(owner) = self.document.token_registers[260].owner.clone() {
            self.push_tokens(owner, 0)?;
        }
        Ok(())
    }
    fn finish_inline_math(&mut self) -> Result<()> {
        if self.current_group() != Some(GroupKind::MathShift) {
            return Err(Error::Group);
        }
        let frame = self.document.math_frames.pop().expect("inline formula");
        let Context::Inline(outer) = frame.context else {
            unreachable!("math-shift owner")
        };
        debug_assert_eq!(frame.level, self.document.level);
        self.document.text = outer;
        let width = self.document.layout_values.dimens[DimenParam::MathSurround.index()].value;
        let penalties = self.document.text.mode == Mode::Horizontal;
        let list = self
            .math
            .as_mut()
            .unwrap()
            .finish_list(frame.list, frame.incomplete, None);
        let output = self.convert_math(list, Style::TEXT, penalties)?;
        if let Err(error) = self.append_horizontal(nodes::Kind::MathBoundary {
            width,
            after: false,
        }) {
            self.document.nodes.release(output, &mut self.page_release);
            return Err(error);
        }
        self.document
            .nodes
            .splice(&mut self.document.text.horizontal, output);
        self.append_horizontal(nodes::Kind::MathBoundary { width, after: true })?;
        self.document.text.space_factor = 1000;
        self.close_group(GroupKind::MathShift)
    }
    pub(super) fn convert_math(
        &mut self,
        list: MathList,
        style: Style,
        penalties: bool,
    ) -> Result<nodes::List> {
        // Both reusable objects are restored on every Result path.
        let mut math = self.math.take().expect("math store outside conversion");
        let mut scratch = self.math_scratch.take().expect("conversion scratch");
        let result = self.math_to_hlist(&mut math, list, style, penalties, &mut scratch);
        self.math = Some(math);
        self.math_scratch = Some(scratch);
        result
    }
    pub(crate) fn finish_math_field(&mut self) -> Result<()> {
        let frame = self.document.math_frames.pop().expect("subsidiary formula");
        let Context::Field(target) = frame.context else {
            unreachable!("math field owner")
        };
        debug_assert_eq!(frame.level, self.document.level + 1);
        self.document.text.mode = frame.enclosing_mode;
        let math = self.math.as_mut().unwrap();
        let list = math.finish_list(frame.list, frame.incomplete, None);
        let tail = self.document.math_frames.last().unwrap().list.tail();
        math.finish_field(target, list, tail, &mut self.document.nodes)
    }
}
fn class(code: i32) -> Class {
    [
        Class::Ord,
        Class::Op,
        Class::Bin,
        Class::Rel,
        Class::Open,
        Class::Close,
        Class::Punct,
        Class::Inner,
    ][code as usize]
}
#[path = "math_choice_frontend.rs"]
mod choices;
#[path = "math_delimiter_frontend.rs"]
mod delimiters;
#[path = "math_display.rs"]
mod display;
#[path = "math_forms_frontend.rs"]
mod forms;
#[path = "math_fraction_frontend.rs"]
mod fractions;
#[path = "math_scan.rs"]
mod scan;
#[cfg(test)]
#[path = "math_frontend_tests.rs"]
mod tests;
