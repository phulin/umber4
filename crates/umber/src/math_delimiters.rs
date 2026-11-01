//! TeX var_delimiter: size/family search and in-place extensible construction.
use super::*;
use crate::layout_values::DimenParam;
impl Conversion<'_, '_, '_> {
    pub(super) fn delimiter(
        &mut self,
        d: Delimiter,
        style: Style,
        target: i32,
    ) -> Result<DetachedNode> {
        let mut best = None;
        let mut extent = 0;
        'search: for (family, character) in [
            (d.small_family, d.small_char),
            (d.large_family, d.large_char),
        ] {
            if family == 0 && character == 0 {
                continue;
            }
            if family > 15 {
                return Err(Error::Font);
            }
            for size in (0..=style.size()).rev() {
                let font = self.engine.document.scalars[Target::family(size, family as usize)].value
                    as u32;
                let Some(scaled) = self.descriptor(font).scaled else {
                    continue;
                };
                let mut c = character;
                while let Some(m) = self.engine.character_metrics(font, c) {
                    let r = self.engine.font_catalog.scaled[scaled].source();
                    if let Some(recipe) = r.extensible_recipe(c) {
                        best = Some((font, c, Some(recipe)));
                        break 'search;
                    }
                    let current = m.height.0.wrapping_add(m.depth.0);
                    if current > extent {
                        best = Some((font, c, None));
                        extent = current;
                        if extent >= target {
                            break 'search;
                        }
                    }
                    let Some(next) = r.larger_character(c) else {
                        break;
                    };
                    c = next;
                }
            }
        }
        let b = match best {
            Some((font, _, Some(recipe))) => self.extensible(font, recipe, target)?,
            Some((font, c, None)) => self.char_box(font, c)?,
            None => self.engine.document.nodes.alloc_box(BoxData {
                width: self.engine.document.layout_values.dimens
                    [DimenParam::NullDelimiterSpace.index()]
                .value,
                ..BoxData::default()
            })?,
        };
        let axis = self.symbol(style, 22);
        let data = self.engine.document.nodes.box_mut(b.id());
        data.shift = Scaled(half(data.height.0.wrapping_sub(data.depth.0)).wrapping_sub(axis));
        Ok(b)
    }
    fn extensible(&mut self, font: u32, recipe: [u8; 4], target: i32) -> Result<DetachedNode> {
        let [top, middle, bottom, repeat] = recipe;
        let metrics = |c| {
            self.engine
                .character_metrics(font, c)
                .expect("validated extensible piece")
        };
        let m = metrics(repeat);
        let unit = m.height.0.wrapping_add(m.depth.0);
        let width = Scaled(m.width.0.wrapping_add(m.italic.0));
        let mut total = 0i32;
        for c in [bottom, middle, top] {
            if c != 0 {
                let m = metrics(c);
                total = total.wrapping_add(m.height.0.wrapping_add(m.depth.0));
            }
        }
        let mut count = 0usize;
        if unit > 0 {
            while total < target {
                total = total.wrapping_add(unit);
                count += 1;
                if middle != 0 {
                    total = total.wrapping_add(unit);
                }
            }
        }
        let mut list = List::default();
        let mut height = 0;
        let result = (|| {
            self.stack_piece(&mut list, &mut height, font, top, top != 0)?;
            for _ in 0..count {
                self.stack_piece(&mut list, &mut height, font, repeat, true)?;
            }
            self.stack_piece(&mut list, &mut height, font, middle, middle != 0)?;
            if middle != 0 {
                for _ in 0..count {
                    self.stack_piece(&mut list, &mut height, font, repeat, true)?;
                }
            }
            self.stack_piece(&mut list, &mut height, font, bottom, bottom != 0)?;
            self.engine.document.nodes.alloc_box(BoxData {
                list: std::mem::take(&mut list),
                vertical: true,
                width,
                height: Scaled(height),
                depth: Scaled(total.wrapping_sub(height)),
                ..BoxData::default()
            })
        })();
        self.release_value(Value::List(list));
        result
    }
    fn stack_piece(
        &mut self,
        list: &mut List,
        height: &mut i32,
        font: u32,
        character: u8,
        present: bool,
    ) -> Result<()> {
        if !present {
            return Ok(());
        }
        let b = self.char_box(font, character)?;
        if list.is_empty() {
            *height = self.engine.document.nodes.box_mut(b.id()).height.0;
        }
        self.engine.document.nodes.append(list, b);
        Ok(())
    }
}
