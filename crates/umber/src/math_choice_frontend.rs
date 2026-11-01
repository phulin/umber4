//! Execute every branch; conversion alone selects a style's owned list.
use super::*;

impl Engine<'_> {
    pub(super) fn math_choice(&mut self) -> Result<()> {
        let atom = self.append_math(Kind::Choice(std::array::from_fn(|_| MathList::default())))?;
        self.begin_math_choice_branch(atom, 0)
    }
    fn begin_math_choice_branch(&mut self, atom: MathId, branch: u8) -> Result<()> {
        self.open_group(GroupKind::MathChoice)?;
        self.document.math_frames.push(Frame {
            list: MathList::default(),
            incomplete: None,
            context: Context::Choice { atom, branch },
            level: self.document.level,
        });
        // WEB push_math precedes scan_left_brace: expansion observes the new
        // group, including aftergroup tokens from the preceding branch.
        loop {
            let read = self.nonspace(true)?;
            match read.meaning.command {
                cmd::RELAX => continue,
                cmd::BEGIN => return Ok(()),
                _ => {
                    self.back(read.token);
                    return Err(Error::Syntax);
                }
            }
        }
    }
    pub(crate) fn finish_math_choice_branch(&mut self) -> Result<()> {
        let frame = self.document.math_frames.pop().expect("choice branch");
        let Context::Choice { atom, branch } = frame.context else {
            unreachable!("math choice group")
        };
        debug_assert_eq!(frame.level, self.document.level + 1);
        let math = self.math.as_mut().unwrap();
        let list = math.finish_list(frame.list, frame.incomplete, None);
        math.finish_choice_branch(atom, branch, list);
        if branch < 3 {
            self.begin_math_choice_branch(atom, branch + 1)?;
        }
        Ok(())
    }
}
