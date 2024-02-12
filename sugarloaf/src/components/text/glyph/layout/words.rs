// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use crate::components::text::glyph::layout::{
    characters::{Character, Characters},
    linebreak::{LineBreak, LineBreaker},
    lines::Lines,
    SectionGlyph, SectionText,
};
use ab_glyph::*;
use std::iter::{FusedIterator, Iterator, Peekable};

#[derive(Clone, Debug, Default)]
pub(crate) struct VMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
}

impl VMetrics {
    #[inline]
    pub fn height(&self) -> f32 {
        self.ascent - self.descent + self.line_gap
    }

    #[inline]
    pub fn max(self, other: Self) -> Self {
        if other.height() > self.height() {
            other
        } else {
            self
        }
    }
}

impl<F: Font> From<PxScaleFont<F>> for VMetrics {
    #[inline]
    fn from(scale_font: PxScaleFont<F>) -> Self {
        Self {
            ascent: scale_font.ascent(),
            descent: scale_font.descent(),
            line_gap: scale_font.line_gap(),
        }
    }
}

/// Single 'word' ie a sequence of `Character`s where the last is a line-break.
///
/// Glyphs are relatively positioned from (0, 0) in a left-top alignment style.
pub(crate) struct Word {
    pub glyphs: Vec<SectionGlyph>,
    /// pixel advance width of word includes ending spaces/invisibles
    pub layout_width: f32,
    /// pixel advance width of word not including any trailing spaces/invisibles
    pub layout_width_no_trail: f32,
    pub max_v_metrics: VMetrics,
    /// indicates the break after the word is a hard one
    pub hard_break: bool,
}

/// `Word` iterator.
pub(crate) struct Words<'a, 'b, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    pub(crate) characters: Peekable<Characters<'a, 'b, L, F, S>>,
}

impl<'a, 'b, L, F, S> Words<'a, 'b, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    pub(crate) fn lines(self, width_bound: f32) -> Lines<'a, 'b, L, F, S> {
        Lines {
            words: self.peekable(),
            width_bound,
        }
    }
}

impl<'a, L, F, S> Iterator for Words<'a, '_, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    type Item = Word;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let mut glyphs = Vec::new();
        let mut caret = 0.0;
        let mut caret_no_trail = caret;
        let mut last_glyph_id = None;
        let mut max_v_metrics = VMetrics::default();
        let mut hard_break = false;
        let mut progress = false;

        for Character {
            mut glyph,
            scale_font,
            font_id,
            line_break,
            control,
            whitespace,
            section_index,
            byte_index,
        } in &mut self.characters
        {
            progress = true;

            max_v_metrics = max_v_metrics.max(scale_font.into());

            if let Some(id) = last_glyph_id.take() {
                caret += scale_font.kern(id, glyph.id);
            }
            last_glyph_id = Some(glyph.id);

            if !control {
                let advance_width = scale_font.h_advance(glyph.id);

                glyph.position = point(caret, 0.0);
                glyphs.push(SectionGlyph {
                    section_index,
                    byte_index,
                    glyph,
                    font_id,
                });
                caret += advance_width;

                if !whitespace {
                    // not an invisible trail
                    caret_no_trail = caret;
                }
            }

            if let Some(lbreak) = line_break {
                // simulate hard-break at end of all sections
                if matches!(lbreak, LineBreak::Hard(_))
                    || self.characters.peek().is_none()
                {
                    hard_break = true
                }
                break;
            }
        }

        if progress {
            return Some(Word {
                glyphs,
                layout_width: caret,
                layout_width_no_trail: caret_no_trail,
                hard_break,
                max_v_metrics,
            });
        }

        None
    }
}

impl<'a, L, F, S> FusedIterator for Words<'a, '_, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
}
