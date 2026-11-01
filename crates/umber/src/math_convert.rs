//! Iterative TeX mlist first/second passes and clean-box continuation work.
use super::*;
use crate::nodes::KernKind;
use crate::{
    Engine,
    fonts::Font,
    layout_values::REGISTERS,
    nodes::{self, BoxData, GlueKind},
    numeric::{Glue, GlueOrder, Scaled},
    packing::PackSpec,
    scalars::Target,
    tfm::Metrics,
};

pub(crate) struct Scratch {
    tasks: Buffer<Task>,
    result: Option<Value>,
    release: ReleaseScratch,
    #[cfg(test)]
    fail_after: Option<usize>,
}
impl Default for Scratch {
    fn default() -> Self {
        Self {
            tasks: Buffer::new(&std::rc::Rc::default()),
            result: None,
            release: ReleaseScratch::default(),
            #[cfg(test)]
            fail_after: None,
        }
    }
}
#[cfg(test)]
impl Scratch {
    pub(super) fn interrupt_after(&mut self, steps: usize) {
        self.fail_after = Some(steps);
    }
    pub(super) fn clear_interrupt(&mut self) {
        self.fail_after = None;
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.tasks.is_empty() && self.result.is_none()
    }
}
enum Value {
    List(List),
    Box(DetachedNode),
}
impl Value {
    fn list(self) -> List {
        if let Self::List(l) = self {
            l
        } else {
            unreachable!("math list result")
        }
    }
    fn boxed(self) -> DetachedNode {
        if let Self::Box(b) = self {
            b
        } else {
            unreachable!("clean box result")
        }
    }
}
enum Action {
    Again,
    Child(Task),
    Replace(Task),
    Done(Value),
}
enum Task {
    List(ListWork),
    Atom(AtomWork),
    Clean(CleanWork),
    Form(forms::FormWork),
    Fraction(fraction::FractionWork),
    Accent(accents::AccentWork),
}
#[derive(Clone, Copy)]
enum Pass {
    First,
    Atom,
    Second,
}
struct ListWork {
    list: MathList,
    cursor: MathId,
    previous: MathId,
    previous_class: Option<Class>,
    style: Style,
    initial: Style,
    penalties: bool,
    delimited: bool,
    closed: bool,
    max_height: i32,
    max_depth: i32,
    pass: Pass,
    output: List,
    pending: List,
}
impl ListWork {
    fn new(list: MathList, style: Style, penalties: bool) -> Self {
        Self {
            cursor: list.head,
            list,
            previous: MathId::NULL,
            previous_class: Some(Class::Op),
            style,
            initial: style,
            penalties,
            delimited: false,
            closed: false,
            max_height: 0,
            max_depth: 0,
            pass: Pass::First,
            output: List::default(),
            pending: List::default(),
        }
    }
}
struct CleanWork {
    field: Field,
    style: Style,
    waiting: bool,
}
impl CleanWork {
    fn task(field: Field, style: Style) -> Task {
        Task::Clean(Self {
            field,
            style,
            waiting: false,
        })
    }
}
#[derive(Clone, Copy)]
enum AtomStage {
    Nucleus,
    NucleusReady,
    Scripts,
    SupReady,
    Sub,
    SubReady,
    Finish,
}
struct AtomWork {
    atom: Atom,
    style: Style,
    stage: AtomStage,
    limits: bool,
    op_clean: bool,
    op_character: bool,
    has_sup: bool,
    has_sub: bool,
    delta: Scaled,
    up: i32,
    down: i32,
    output: List,
    temp: List,
    base: Option<DetachedNode>,
    upper: Option<DetachedNode>,
    lower: Option<DetachedNode>,
}
impl AtomWork {
    fn new(atom: Atom, style: Style) -> Self {
        Self {
            limits: atom.class == Class::Op
                && (atom.limits == Limits::AboveBelow
                    || (atom.limits == Limits::Normal && style.display())),
            op_clean: atom.class == Class::Op
                && (matches!(atom.fields.nucleus, Field::Character { .. })
                    || atom.limits == Limits::AboveBelow
                    || (atom.limits == Limits::Normal && style.display())),
            op_character: atom.class == Class::Op
                && matches!(atom.fields.nucleus, Field::Character { .. }),
            has_sup: !atom.fields.sup.is_empty(),
            has_sub: !atom.fields.sub.is_empty(),
            atom,
            style,
            stage: AtomStage::Nucleus,
            delta: Scaled::ZERO,
            up: 0,
            down: 0,
            output: List::default(),
            temp: List::default(),
            base: None,
            upper: None,
            lower: None,
        }
    }
}
struct Conversion<'e, 'a, 'm> {
    engine: &'e mut Engine<'a>,
    math: &'m mut MathStore,
    scratch: &'m mut Scratch,
}
impl Engine<'_> {
    pub(crate) fn math_to_hlist(
        &mut self,
        math: &mut MathStore,
        list: MathList,
        style: Style,
        penalties: bool,
        scratch: &mut super::Scratch,
    ) -> Result<List> {
        assert!(scratch.tasks.is_empty() && scratch.result.is_none());
        scratch
            .tasks
            .push(Task::List(ListWork::new(list, style, penalties)));
        let mut c = Conversion {
            engine: self,
            math,
            scratch,
        };
        let result = c.engine.validate_math_fonts().and_then(|()| c.run());
        if result.is_err() {
            c.cleanup();
        }
        result
    }
}
impl Engine<'_> {
    pub(crate) fn validate_math_fonts(&self) -> Result<()> {
        for size in 0..3 {
            for (family, count) in [(2, 22), (3, 13)] {
                let id = self.document.scalars[Target::family(size, family)].value as usize;
                if self.document.fonts.descriptors[id].count < count {
                    return Err(Error::Font);
                }
            }
        }
        Ok(())
    }
}
impl Conversion<'_, '_, '_> {
    fn run(&mut self) -> Result<List> {
        while let Some(mut task) = self.scratch.tasks.pop() {
            // Test every ownership handoff without imposing a production limit.
            #[cfg(test)]
            if let Some(left) = &mut self.scratch.fail_after {
                if *left == 0 {
                    self.release_task(task);
                    return Err(Error::Capacity);
                }
                *left -= 1;
            }
            let step = match &mut task {
                Task::List(w) => self.list_step(w),
                Task::Atom(w) => self.atom_step(w),
                Task::Clean(w) => self.clean_step(w),
                Task::Form(w) => self.form_step(w),
                Task::Fraction(w) => self.fraction_step(w),
                Task::Accent(w) => self.accent_step(w),
            };
            match step {
                Ok(Action::Again) => self.scratch.tasks.push(task),
                Ok(Action::Child(child)) => {
                    self.scratch.tasks.push(task);
                    self.scratch.tasks.push(child);
                }
                Ok(Action::Replace(next)) => {
                    self.release_task(task);
                    self.scratch.tasks.push(next);
                }
                Ok(Action::Done(value)) => {
                    self.release_task(task);
                    assert!(self.scratch.result.is_none());
                    self.scratch.result = Some(value);
                }
                Err(error) => {
                    self.release_task(task);
                    return Err(error);
                }
            }
        }
        Ok(self.scratch.result.take().expect("root math result").list())
    }
    fn result(&mut self) -> Value {
        self.scratch.result.take().expect("child math result")
    }
    fn release_value(&mut self, value: Value) {
        match value {
            Value::List(l) => self
                .engine
                .document
                .nodes
                .release(l, &mut self.scratch.release),
            Value::Box(b) => self
                .engine
                .document
                .nodes
                .release_node(b, &mut self.scratch.release),
        }
    }
    fn release_task(&mut self, task: Task) {
        match task {
            Task::Form(w) => self.release_form(w),
            Task::Fraction(w) => self.release_fraction(w),
            Task::Accent(w) => self.release_accent(w),
            Task::List(w) => {
                self.math.release(w.list, &mut self.engine.document.nodes);
                self.release_value(Value::List(w.output));
                self.release_value(Value::List(w.pending));
            }
            Task::Clean(w) => {
                self.math
                    .release_field(w.field, &mut self.engine.document.nodes);
                self.math.drain_release(&mut self.engine.document.nodes);
            }
            Task::Atom(w) => {
                self.math
                    .release_kind(Kind::Atom(w.atom), &mut self.engine.document.nodes);
                self.release_value(Value::List(w.output));
                self.release_value(Value::List(w.temp));
                for b in [w.base, w.upper, w.lower].into_iter().flatten() {
                    self.release_value(Value::Box(b));
                }
            }
        }
    }
    fn cleanup(&mut self) {
        if let Some(v) = self.scratch.result.take() {
            self.release_value(v);
        }
        while let Some(t) = self.scratch.tasks.pop() {
            self.release_task(t);
        }
    }
    fn cancel_binary(&mut self, id: MathId, class: Option<Class>) {
        if class == Some(Class::Bin) {
            self.math.node_mut(id).kind.set_class(Class::Ord);
        }
    }
    fn font(&self, family: u8, style: Style) -> Result<u32> {
        if family > 15 {
            return Err(Error::Font);
        }
        let f = self.engine.document.scalars[Target::family(style.size(), family as usize)].value
            as usize;
        if self.engine.document.fonts.descriptors[f].scaled.is_none() {
            return Err(Error::Font);
        }
        Ok(f as u32)
    }
    fn glyph(&self, family: u8, character: u8, style: Style) -> Result<Option<(u32, Metrics)>> {
        let font = self.font(family, style)?;
        Ok(self
            .engine
            .character_metrics(font, character)
            .map(|m| (font, m)))
    }
    fn descriptor(&self, font: u32) -> Font {
        self.engine.document.fonts.descriptors[font as usize]
    }
    fn parameter(&self, family: usize, size: usize, parameter: usize) -> i32 {
        let font = self.engine.document.scalars[Target::family(size, family)].value as usize;
        self.engine.font_parameter(font, parameter).0
    }
    fn symbol(&self, style: Style, parameter: usize) -> i32 {
        self.parameter(2, style.size(), parameter)
    }
    fn extension(&self, style: Style, parameter: usize) -> i32 {
        self.parameter(3, style.size(), parameter)
    }
    fn append(&mut self, list: &mut List, kind: nodes::Kind) -> Result<()> {
        let n = self.engine.document.nodes.alloc(kind)?;
        self.engine.document.nodes.append(list, n);
        Ok(())
    }
    fn kern(&mut self, list: &mut List, width: i32) -> Result<()> {
        self.append(
            list,
            nodes::Kind::Kern {
                width: Scaled(width),
                kind: KernKind::Normal,
            },
        )
    }
    fn spacing(
        &mut self,
        output: &mut List,
        before: Class,
        after: Class,
        style: Style,
    ) -> Result<()> {
        const TABLE: &[u8; 64] =
            b"0234000122*4000133**3**344*0400400*000000234000111*1111112341011";
        let code = TABLE[before as usize * 8 + after as usize];
        let index = match code {
            b'0' => return Ok(()),
            b'1' if style.script() => return Ok(()),
            b'3' | b'4' if style.script() => return Ok(()),
            b'1' | b'2' => 0,
            b'3' => 1,
            b'4' => 2,
            _ => unreachable!("binary context cancellation"),
        };
        let spec = self.engine.document.layout_values.mu_glues[REGISTERS + index].value;
        let spec = Glue {
            width: self.mu_dimension(style, spec.width)?,
            stretch: if spec.stretch_order == GlueOrder::Normal {
                self.mu_dimension(style, spec.stretch)?
            } else {
                spec.stretch
            },
            shrink: if spec.shrink_order == GlueOrder::Normal {
                self.mu_dimension(style, spec.shrink)?
            } else {
                spec.shrink
            },
            ..spec
        };
        self.append(
            output,
            nodes::Kind::Glue {
                spec,
                subtype: GlueKind::Normal,
            },
        )
    }
    fn mu_dimension(&self, style: Style, value: Scaled) -> Result<Scaled> {
        let mu = self.symbol(style, 6) / 18;
        Scaled::checked(
            i64::from(mu.div_euclid(65536)) * i64::from(value.0)
                + i64::from(value.0) * i64::from(mu.rem_euclid(65536)) / 65536,
        )
    }
}
pub(crate) fn half(value: i32) -> i32 {
    value / 2 + i32::from(value % 2 > 0)
}
#[path = "math_atoms.rs"]
mod atoms;

#[path = "math_clean.rs"]
mod clean;

#[path = "math_lists.rs"]
mod lists;

#[path = "math_boxing.rs"]
mod boxing;
#[path = "math_scripts.rs"]
mod scripts;

#[path = "math_delimiters.rs"]
mod delimiters;
#[path = "math_forms.rs"]
mod forms;
#[path = "math_fraction.rs"]
mod fraction;

#[path = "math_accents.rs"]
mod accents;
