// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use crate::components::text::glyph::layout::{
    linebreak::{EolLineBreak, LineBreak, LineBreaker},
    words::Words,
    FontId, SectionText,
};
use ab_glyph::*;
use std::{
    iter::{Enumerate, FusedIterator, Iterator},
    str::CharIndices,
};

/// Single character info
pub(crate) struct Character<'b, F: Font> {
    pub glyph: Glyph,
    pub scale_font: PxScaleFont<&'b F>,
    pub font_id: FontId,
    /// Line break proceeding this character.
    pub line_break: Option<LineBreak>,
    /// Equivalent to `char::is_control()`.
    pub control: bool,
    /// Equivalent to `char::is_whitespace()`.
    pub whitespace: bool,
    /// Index of the `SectionText` this character is from.
    pub section_index: usize,
    /// Position of the char within the `SectionText` text.
    pub byte_index: usize,
}

/// `Character` iterator
pub(crate) struct Characters<'a, 'b, L, F, S>
where
    F: Font,
    L: LineBreaker,
    S: Iterator<Item = SectionText<'a>>,
{
    fonts: &'b [F],
    section_text: Enumerate<S>,
    line_breaker: L,
    part_info: Option<PartInfo<'a>>,
}

struct PartInfo<'a> {
    section_index: usize,
    section: SectionText<'a>,
    info_chars: CharIndices<'a>,
    line_breaks: Box<dyn Iterator<Item = LineBreak> + 'a>,
    next_break: Option<LineBreak>,
}

impl<'a, 'b, L, F, S> Characters<'a, 'b, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    /// Returns a new `Characters` iterator.
    pub(crate) fn new(fonts: &'b [F], section_text: S, line_breaker: L) -> Self {
        Self {
            fonts,
            section_text: section_text.enumerate(),
            line_breaker,
            part_info: None,
        }
    }

    /// Wraps into a `Words` iterator.
    pub(crate) fn words(self) -> Words<'a, 'b, L, F, S> {
        Words {
            characters: self.peekable(),
        }
    }
}

impl<'a, 'b, L, F, S> Iterator for Characters<'a, 'b, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    type Item = Character<'b, F>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.part_info.is_none() {
            let mut index_and_section;
            loop {
                index_and_section = self.section_text.next()?;
                if valid_section(&index_and_section.1) {
                    break;
                }
            }
            let (section_index, section) = index_and_section;
            let line_breaks = self.line_breaker.line_breaks(section.text);
            self.part_info = Some(PartInfo {
                section_index,
                section,
                info_chars: index_and_section.1.text.char_indices(),
                line_breaks,
                next_break: None,
            });
        }

        {
            let PartInfo {
                section_index,
                section:
                    SectionText {
                        scale,
                        font_id,
                        text,
                    },
                info_chars,
                line_breaks,
                next_break,
            } = self.part_info.as_mut().unwrap();

            if let Some((byte_index, c)) = info_chars.next() {
                if next_break.is_none() || next_break.unwrap().offset() <= byte_index {
                    loop {
                        let next = line_breaks.next();
                        if next.is_none() || next.unwrap().offset() > byte_index {
                            *next_break = next;
                            break;
                        }
                    }
                }

                let scale_font: PxScaleFont<&'b F> =
                    self.fonts[*font_id].as_scaled(*scale);

                let glyph = scale_font.scaled_glyph(c);
                // println!("{:?} {:?}", c, scale);

                let c_len = c.len_utf8();
                let mut line_break =
                    next_break.filter(|b| b.offset() == byte_index + c_len);
                if line_break.is_some() && byte_index + c_len == text.len() {
                    // handle inherent end-of-str breaks
                    line_break = line_break.and(c.eol_line_break(&self.line_breaker));
                }

                return Some(Character {
                    glyph,
                    scale_font,
                    font_id: *font_id,
                    line_break,
                    control: c.is_control(),
                    whitespace: c.is_whitespace(),
                    section_index: *section_index,
                    byte_index,
                });
            }
        }

        self.part_info = None;
        self.next()
    }
}

impl<'a, L, F, S> FusedIterator for Characters<'a, '_, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
}

#[inline]
fn valid_section(s: &SectionText<'_>) -> bool {
    let PxScale { x, y } = s.scale;
    x > 0.0 && y > 0.0
}
