// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use super::{HorizontalAlign, SectionGlyph, SectionText, VerticalAlign};
use crate::components::text::glyph::layout::{linebreak::LineBreaker, words::*};
use ab_glyph::*;
use std::iter::{FusedIterator, Iterator, Peekable};

/// A line of `Word`s limited to a max width bound.
#[derive(Default)]
pub(crate) struct Line {
    pub glyphs: Vec<SectionGlyph>,
    pub max_v_metrics: VMetrics,
    pub rightmost: f32,
}

impl Line {
    #[inline]
    pub(crate) fn line_height(&self) -> f32 {
        self.max_v_metrics.ascent - self.max_v_metrics.descent
            + self.max_v_metrics.line_gap
    }

    /// Returns line glyphs positioned on the screen and aligned.
    #[inline]
    pub fn aligned_on_screen(
        mut self,
        (screen_x, screen_y): (f32, f32),
        h_align: HorizontalAlign,
        v_align: VerticalAlign,
    ) -> Vec<SectionGlyph> {
        if self.glyphs.is_empty() {
            return Vec::new();
        }

        // implement v-aligns when they're are supported
        let screen_left = match h_align {
            HorizontalAlign::Left => point(screen_x, screen_y),
            // - Right alignment attained from left by shifting the line
            //   leftwards by the rightmost x distance from render position
            // - Central alignment is attained from left by shifting the line
            //   leftwards by half the rightmost x distance from render position
            HorizontalAlign::Center | HorizontalAlign::Right => {
                let mut shift_left = self.rightmost;
                if h_align == HorizontalAlign::Center {
                    shift_left /= 2.0;
                }
                point(screen_x - shift_left, screen_y)
            }
        };

        let screen_pos = match v_align {
            VerticalAlign::Top => screen_left,
            VerticalAlign::Center => {
                let mut screen_pos = screen_left;
                screen_pos.y -= self.line_height() / 2.0;
                screen_pos
            }
            VerticalAlign::Bottom => {
                let mut screen_pos = screen_left;
                screen_pos.y -= self.line_height();
                screen_pos
            }
        };

        self.glyphs
            .iter_mut()
            .for_each(|sg| sg.glyph.position += screen_pos);

        self.glyphs
    }
}

/// `Line` iterator.
///
/// Will iterator through `Word` until the next word would break the `width_bound`.
///
/// Note: Will always have at least one word, if possible, even if the word itself
/// breaks the `width_bound`.
pub(crate) struct Lines<'a, 'b, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    pub(crate) words: Peekable<Words<'a, 'b, L, F, S>>,
    pub(crate) width_bound: f32,
}

impl<'a, L, F, S> Iterator for Lines<'a, '_, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
    type Item = Line;

    fn next(&mut self) -> Option<Self::Item> {
        let mut caret = point(0.0, 0.0);
        let mut line = Line::default();

        let mut progressed = false;

        while let Some(word) = self.words.peek() {
            // Drop trailing spaces when bounds-wrapping.
            // However, if the word ends in a hard-break "Foo  \n" keep the trailing space width.
            let word_wrap_width = match word.hard_break {
                false => word.layout_width_no_trail,
                true => word.layout_width,
            };

            let word_right = caret.x + word_wrap_width;
            // Reduce float errors by using relative "<= width bound" check
            let word_in_bounds = word_right < self.width_bound
                || approx::relative_eq!(word_right, self.width_bound);

            // only if `progressed` means the first word is allowed to overlap the bounds
            if !word_in_bounds && progressed {
                break;
            }

            let word = self.words.next().unwrap();
            progressed = true;

            line.rightmost = word_right;

            if (line.glyphs.is_empty() || !word.glyphs.is_empty())
                && word.max_v_metrics.height() > line.max_v_metrics.height()
            {
                let diff_y = word.max_v_metrics.ascent - caret.y;
                caret.y += diff_y;

                // modify all smaller lined glyphs to occupy the new larger line
                for SectionGlyph { glyph, .. } in &mut line.glyphs {
                    glyph.position.y += diff_y;
                }

                line.max_v_metrics = word.max_v_metrics;
            }

            line.glyphs.extend(word.glyphs.into_iter().map(|mut sg| {
                sg.glyph.position += caret;
                sg
            }));

            caret.x += word.layout_width;

            if word.hard_break {
                break;
            }
        }

        Some(line).filter(|_| progressed)
    }
}

impl<'a, L, F, S> FusedIterator for Lines<'a, '_, L, F, S>
where
    L: LineBreaker,
    F: Font,
    S: Iterator<Item = SectionText<'a>>,
{
}
