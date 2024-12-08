// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use super::{
    BuiltInLineBreaker, GlyphPositioner, LineBreaker, SectionGeometry, ToSectionText,
};
use crate::components::text::glyph::layout::{
    characters::Characters, GlyphChange, SectionGlyph,
};
use crate::components::text::glyph::Layout::SingleLine;
use crate::components::text::glyph::Layout::Wrap;
use ab_glyph::*;

/// Built-in [`GlyphPositioner`](trait.GlyphPositioner.html) implementations.
///
/// Takes generic [`LineBreaker`](trait.LineBreaker.html) to indicate the wrapping style.
/// See [`BuiltInLineBreaker`](enum.BuiltInLineBreaker.html).
///
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Layout<L: LineBreaker> {
    /// Renders a single line from left-to-right according to the inner alignment.
    /// Hard breaking will end the line, partially hitting the width bound will end the line.
    SingleLine {
        line_breaker: L,
        h_align: HorizontalAlign,
        v_align: VerticalAlign,
    },
    /// Renders multiple lines from left-to-right according to the inner alignment.
    /// Hard breaking characters will cause advancement to another line.
    /// A characters hitting the width bound will also cause another line to start.
    Wrap {
        line_breaker: L,
        h_align: HorizontalAlign,
        v_align: VerticalAlign,
    },
}

impl Default for Layout<BuiltInLineBreaker> {
    #[inline]
    fn default() -> Self {
        Layout::default_wrap()
    }
}

impl Layout<BuiltInLineBreaker> {
    #[inline]
    pub fn default_single_line() -> Self {
        Layout::SingleLine {
            line_breaker: BuiltInLineBreaker::default(),
            h_align: HorizontalAlign::Left,
            v_align: VerticalAlign::Top,
        }
    }

    #[inline]
    pub fn default_wrap() -> Self {
        Layout::Wrap {
            line_breaker: BuiltInLineBreaker::default(),
            h_align: HorizontalAlign::Left,
            v_align: VerticalAlign::Top,
        }
    }
}

impl<L: LineBreaker> Layout<L> {
    /// Returns an identical `Layout` but with the input `h_align`
    pub fn h_align(self, h_align: HorizontalAlign) -> Self {
        use crate::components::text::glyph::Layout::*;
        match self {
            SingleLine {
                line_breaker,
                v_align,
                ..
            } => SingleLine {
                line_breaker,
                v_align,
                h_align,
            },
            Wrap {
                line_breaker,
                v_align,
                ..
            } => Wrap {
                line_breaker,
                v_align,
                h_align,
            },
        }
    }

    /// Returns an identical `Layout` but with the input `v_align`
    pub fn v_align(self, v_align: VerticalAlign) -> Self {
        use crate::components::text::glyph::Layout::*;
        match self {
            SingleLine {
                line_breaker,
                h_align,
                ..
            } => SingleLine {
                line_breaker,
                v_align,
                h_align,
            },
            Wrap {
                line_breaker,
                h_align,
                ..
            } => Wrap {
                line_breaker,
                v_align,
                h_align,
            },
        }
    }

    /// Returns an identical `Layout` but with the input `line_breaker`
    pub fn line_breaker<L2: LineBreaker>(self, line_breaker: L2) -> Layout<L2> {
        use crate::components::text::glyph::Layout::*;
        match self {
            SingleLine {
                h_align, v_align, ..
            } => SingleLine {
                line_breaker,
                v_align,
                h_align,
            },
            Wrap {
                h_align, v_align, ..
            } => Wrap {
                line_breaker,
                v_align,
                h_align,
            },
        }
    }
}

impl<L: LineBreaker> GlyphPositioner for Layout<L> {
    fn calculate_glyphs<F, S>(
        &self,
        fonts: &[F],
        geometry: &SectionGeometry,
        sections: &[S],
    ) -> Vec<SectionGlyph>
    where
        F: Font,
        S: ToSectionText,
    {
        let SectionGeometry {
            screen_position,
            bounds: (bound_w, bound_h),
            ..
        } = *geometry;

        match *self {
            SingleLine {
                h_align,
                v_align,
                line_breaker,
            } => Characters::new(
                fonts,
                sections.iter().map(|s| s.to_section_text()),
                line_breaker,
            )
            .words()
            .lines(bound_w)
            .next()
            .map(|line| line.aligned_on_screen(screen_position, h_align, v_align))
            .unwrap_or_default(),

            Wrap {
                h_align,
                v_align,
                line_breaker,
            } => {
                let mut out = vec![];
                let mut caret = screen_position;
                let v_align_top = v_align == VerticalAlign::Top;

                let lines = Characters::new(
                    fonts,
                    sections.iter().map(|s| s.to_section_text()),
                    line_breaker,
                )
                .words()
                .lines(bound_w);

                for line in lines {
                    // top align can bound check & exit early
                    if v_align_top && caret.1 >= screen_position.1 + bound_h {
                        break;
                    }

                    let line_height = line.line_height();
                    out.extend(line.aligned_on_screen(
                        caret,
                        h_align,
                        VerticalAlign::Top,
                    ));
                    caret.1 += line_height;
                }

                if !out.is_empty() {
                    match v_align {
                        // already aligned
                        VerticalAlign::Top => {}
                        // convert from top
                        VerticalAlign::Center | VerticalAlign::Bottom => {
                            let shift_up = if v_align == VerticalAlign::Center {
                                (caret.1 - screen_position.1) / 2.0
                            } else {
                                caret.1 - screen_position.1
                            };

                            let (min_x, max_x) =
                                h_align.x_bounds(screen_position.0, bound_w);
                            let (min_y, max_y) =
                                v_align.y_bounds(screen_position.1, bound_h);

                            out = out
                                .drain(..)
                                .filter_map(|mut sg| {
                                    // shift into position
                                    sg.glyph.position.y -= shift_up;

                                    // filter away out-of-bounds glyphs
                                    let sfont =
                                        fonts[sg.font_id].as_scaled(sg.glyph.scale);
                                    let h_advance = sfont.h_advance(sg.glyph.id);
                                    let h_side_bearing =
                                        sfont.h_side_bearing(sg.glyph.id);
                                    let height = sfont.height();

                                    Some(sg).filter(|sg| {
                                        sg.glyph.position.x - h_side_bearing <= max_x
                                            && sg.glyph.position.x + h_advance >= min_x
                                            && sg.glyph.position.y - height <= max_y
                                            && sg.glyph.position.y + height >= min_y
                                    })
                                })
                                .collect();
                        }
                    }
                }

                out
            }
        }
    }

    fn bounds_rect(&self, geometry: &SectionGeometry) -> Rect {
        let SectionGeometry {
            screen_position: (screen_x, screen_y),
            bounds: (bound_w, bound_h),
        } = *geometry;

        let (h_align, v_align) = match *self {
            Wrap {
                h_align, v_align, ..
            }
            | SingleLine {
                h_align, v_align, ..
            } => (h_align, v_align),
        };

        let (x_min, x_max) = h_align.x_bounds(screen_x, bound_w);
        let (y_min, y_max) = v_align.y_bounds(screen_y, bound_h);

        Rect {
            min: point(x_min, y_min),
            max: point(x_max, y_max),
        }
    }

    #[allow(clippy::float_cmp)]
    fn recalculate_glyphs<F, S, P>(
        &self,
        previous: P,
        change: GlyphChange,
        fonts: &[F],
        geometry: &SectionGeometry,
        sections: &[S],
    ) -> Vec<SectionGlyph>
    where
        F: Font,
        S: ToSectionText,
        P: IntoIterator<Item = SectionGlyph>,
    {
        match change {
            GlyphChange::Geometry(old) if old.bounds == geometry.bounds => {
                // position change
                let adjustment = point(
                    geometry.screen_position.0 - old.screen_position.0,
                    geometry.screen_position.1 - old.screen_position.1,
                );

                let mut glyphs: Vec<_> = previous.into_iter().collect();
                glyphs
                    .iter_mut()
                    .for_each(|sg| sg.glyph.position += adjustment);
                glyphs
            }
            _ => self.calculate_glyphs(fonts, geometry, sections),
        }
    }
}

/// Describes horizontal alignment preference for positioning & bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HorizontalAlign {
    /// Leftmost character is immediately to the right of the render position.<br/>
    /// Bounds start from the render position and advance rightwards.
    Left,
    /// Leftmost & rightmost characters are equidistant to the render position.<br/>
    /// Bounds start from the render position and advance equally left & right.
    Center,
    /// Rightmost character is immediately to the left of the render position.<br/>
    /// Bounds start from the render position and advance leftwards.
    Right,
}

impl HorizontalAlign {
    #[inline]
    pub(crate) fn x_bounds(self, screen_x: f32, bound_w: f32) -> (f32, f32) {
        let (min, max) = match self {
            HorizontalAlign::Left => (screen_x, screen_x + bound_w),
            HorizontalAlign::Center => {
                (screen_x - bound_w / 2.0, screen_x + bound_w / 2.0)
            }
            HorizontalAlign::Right => (screen_x - bound_w, screen_x),
        };

        (min.floor(), max.ceil())
    }
}

/// Describes vertical alignment preference for positioning & bounds. Currently a placeholder
/// for future functionality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerticalAlign {
    /// Characters/bounds start underneath the render position and progress downwards.
    Top,
    /// Characters/bounds center at the render position and progress outward equally.
    Center,
    /// Characters/bounds start above the render position and progress upward.
    Bottom,
}

impl VerticalAlign {
    #[inline]
    pub(crate) fn y_bounds(self, screen_y: f32, bound_h: f32) -> (f32, f32) {
        let (min, max) = match self {
            VerticalAlign::Top => (screen_y, screen_y + bound_h),
            VerticalAlign::Center => (screen_y - bound_h / 2.0, screen_y + bound_h / 2.0),
            VerticalAlign::Bottom => (screen_y - bound_h, screen_y),
        };

        (min.floor(), max.ceil())
    }
}

#[cfg(test)]
mod bounds_test {
    use super::*;

    #[test]
    fn v_align_y_bounds_inf() {
        assert_eq!(
            VerticalAlign::Top.y_bounds(0.0, f32::INFINITY),
            (0.0, f32::INFINITY)
        );
        assert_eq!(
            VerticalAlign::Center.y_bounds(0.0, f32::INFINITY),
            (-f32::INFINITY, f32::INFINITY)
        );
        assert_eq!(
            VerticalAlign::Bottom.y_bounds(0.0, f32::INFINITY),
            (-f32::INFINITY, 0.0)
        );
    }

    #[test]
    fn h_align_x_bounds_inf() {
        assert_eq!(
            HorizontalAlign::Left.x_bounds(0.0, f32::INFINITY),
            (0.0, f32::INFINITY)
        );
        assert_eq!(
            HorizontalAlign::Center.x_bounds(0.0, f32::INFINITY),
            (-f32::INFINITY, f32::INFINITY)
        );
        assert_eq!(
            HorizontalAlign::Right.x_bounds(0.0, f32::INFINITY),
            (-f32::INFINITY, 0.0)
        );
    }
}

#[cfg(test)]
mod layout_test {
    use super::*;
    use crate::components::text::glyph::layout::{
        BuiltInLineBreaker::*, FontId, SectionText,
    };
    use approx::assert_relative_eq;
    use ordered_float::OrderedFloat;
    use std::sync::LazyLock;
    use std::{collections::*, f32};

    static A_FONT: LazyLock<FontRef<'static>> = LazyLock::new(|| {
        FontRef::try_from_slice(include_bytes!(
            "../../../../../resources/test-fonts/DejaVuSansMono.ttf"
        ))
        .unwrap()
    });
    static CJK_FONT: LazyLock<FontRef<'static>> = LazyLock::new(|| {
        FontRef::try_from_slice(include_bytes!(
            "../../../../../resources/test-fonts/WenQuanYiMicroHei.ttf"
        ))
        .unwrap()
    });
    static FONT_MAP: LazyLock<[&'static FontRef<'static>; 2]> =
        LazyLock::new(|| [&*A_FONT, &*CJK_FONT]);

    /// All the chars used in testing, so we can reverse lookup the glyph-ids
    const TEST_CHARS: &[char] = &[
        'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P',
        'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X', 'Q', 'Z', 'a', 'b', 'c', 'd', 'e', 'f',
        'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v',
        'w', 'x', 'y', 'z', ' ', ',', '.', '提', '高', '代', '碼', '執', '行', '率', '❤',
        'é', 'ß', '\'', '_',
    ];

    /// Turns glyphs into a string, uses `☐` to denote that it didn't work
    fn glyphs_to_common_string<F>(glyphs: &[SectionGlyph], font: &F) -> String
    where
        F: Font,
    {
        glyphs
            .iter()
            .map(|sg| {
                TEST_CHARS
                    .iter()
                    .find(|cc| font.glyph_id(**cc) == sg.glyph.id)
                    .unwrap_or(&'☐')
            })
            .collect()
    }

    /// Checks the order of glyphs in the first arg iterable matches the
    /// second arg string characters
    /// $glyphs: Vec<(Glyph, Color, FontId)>
    macro_rules! assert_glyph_order {
        ($glyphs:expr, $string:expr) => {
            assert_glyph_order!($glyphs, $string, font = &*A_FONT)
        };
        ($glyphs:expr, $string:expr, font = $font:expr) => {{
            assert_eq!($string, glyphs_to_common_string(&$glyphs, $font));
        }};
    }

    /// Compile test for trait stability
    #[allow(unused)]
    #[derive(Hash)]
    enum SimpleCustomGlyphPositioner {}

    impl GlyphPositioner for SimpleCustomGlyphPositioner {
        fn calculate_glyphs<F, S>(
            &self,
            _fonts: &[F],
            _geometry: &SectionGeometry,
            _sections: &[S],
        ) -> Vec<SectionGlyph>
        where
            F: Font,
            S: ToSectionText,
        {
            <_>::default()
        }

        /// Return a screen rectangle according to the requested render position and bounds
        /// appropriate for the glyph layout.
        fn bounds_rect(&self, _: &SectionGeometry) -> Rect {
            Rect {
                min: point(0.0, 0.0),
                max: point(0.0, 0.0),
            }
        }
    }

    #[test]
    fn zero_scale_glyphs() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "hello world",
                    scale: 0.0.into(),
                    ..<_>::default()
                }],
            );

        assert!(glyphs.is_empty(), "{:?}", glyphs);
    }

    #[test]
    fn negative_scale_glyphs() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "hello world",
                    scale: PxScale::from(-20.0),
                    ..<_>::default()
                }],
            );

        assert!(glyphs.is_empty(), "{:?}", glyphs);
    }

    #[test]
    fn single_line_chars_left_simple() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "hello world",
                    scale: PxScale::from(20.0),
                    ..SectionText::default()
                }],
            );

        assert_glyph_order!(glyphs, "hello world");

        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );
    }

    #[test]
    fn single_line_chars_right() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .h_align(HorizontalAlign::Right)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "hello world",
                    scale: PxScale::from(20.0),
                    ..SectionText::default()
                }],
            );

        assert_glyph_order!(glyphs, "hello world");
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(glyphs[0].glyph.position.x < last_glyph.position.x);
        assert!(
            last_glyph.position.x <= 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );

        let sfont = A_FONT.as_scaled(20.0);
        let rightmost_x = last_glyph.position.x + sfont.h_advance(last_glyph.id);
        assert_relative_eq!(rightmost_x, 0.0, epsilon = 1e-1);
    }

    #[test]
    fn single_line_chars_center() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .h_align(HorizontalAlign::Center)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "hello world",
                    scale: PxScale::from(20.0),
                    ..SectionText::default()
                }],
            );

        assert_glyph_order!(glyphs, "hello world");
        assert!(
            glyphs[0].glyph.position.x < 0.0,
            "unexpected first glyph position {:?}",
            glyphs[0].glyph.position
        );

        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last glyph position {:?}",
            last_glyph.position
        );

        let leftmost_x = glyphs[0].glyph.position.x;
        let sfont = A_FONT.as_scaled(20.0);
        let rightmost_x = last_glyph.position.x + sfont.h_advance(last_glyph.id);
        assert_relative_eq!(rightmost_x, -leftmost_x, epsilon = 1e-1);
    }

    #[test]
    fn single_line_chars_left_finish_at_newline() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry::default(),
                &[SectionText {
                    text: "hello\nworld",
                    scale: PxScale::from(20.0),
                    ..SectionText::default()
                }],
            );

        assert_glyph_order!(glyphs, "hello");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        assert!(
            glyphs[4].glyph.position.x > 0.0,
            "unexpected last position {:?}",
            glyphs[4].glyph.position
        );
    }

    #[test]
    fn wrap_word_left() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (85.0, f32::INFINITY), // should only be enough room for the 1st word
                ..SectionGeometry::default()
            },
            &[SectionText {
                text: "hello what's _happening_?",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello ");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );

        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (125.0, f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[SectionText {
                text: "hello what's _happening_?",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "hello what's ");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        let last_glyph = &glyphs.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );
    }

    #[test]
    fn single_line_limited_horizontal_room() {
        let glyphs = Layout::default_single_line()
            .line_breaker(AnyCharLineBreaker)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry {
                    bounds: (50.0, f32::INFINITY),
                    ..SectionGeometry::default()
                },
                &[SectionText {
                    text: "hello world",
                    scale: PxScale::from(20.0),
                    ..SectionText::default()
                }],
            );

        assert_glyph_order!(glyphs, "hell");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
    }

    #[test]
    fn wrap_layout_with_new_lines() {
        let test_str = "Autumn moonlight\n\
                        a worm digs silently\n\
                        into the chestnut.";

        let glyphs = Layout::default_wrap().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: test_str,
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        // newlines don't turn up as glyphs
        assert_glyph_order!(
            glyphs,
            "Autumn moonlighta worm digs silentlyinto the chestnut."
        );
        assert!(
            glyphs[16].glyph.position.y > glyphs[0].glyph.position.y,
            "second line should be lower than first"
        );
        assert!(
            glyphs[36].glyph.position.y > glyphs[16].glyph.position.y,
            "third line should be lower than second"
        );
    }

    #[test]
    fn leftover_max_vmetrics() {
        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (750.0, f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[
                SectionText {
                    text: "Autumn moonlight, ",
                    scale: PxScale::from(30.0),
                    ..SectionText::default()
                },
                SectionText {
                    text: "a worm digs silently ",
                    scale: PxScale::from(40.0),
                    ..SectionText::default()
                },
                SectionText {
                    text: "into the chestnut.",
                    scale: PxScale::from(10.0),
                    ..SectionText::default()
                },
            ],
        );

        for g in glyphs {
            println!("{:?}", (g.glyph.scale, g.glyph.position));
            // all glyphs should have the same ascent drawing position
            let y_pos = g.glyph.position.y;
            assert_relative_eq!(y_pos, A_FONT.as_scaled(40.0).ascent());
        }
    }

    #[test]
    fn eol_new_line_hard_breaks() {
        let glyphs = Layout::default_wrap().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[
                SectionText {
                    text: "Autumn moonlight, \n",
                    ..SectionText::default()
                },
                SectionText {
                    text: "a worm digs silently ",
                    ..SectionText::default()
                },
                SectionText {
                    text: "\n",
                    ..SectionText::default()
                },
                SectionText {
                    text: "into the chestnut.",
                    ..SectionText::default()
                },
            ],
        );

        let y_ords: HashSet<OrderedFloat<f32>> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();

        println!("Y ords: {y_ords:?}");
        assert_eq!(y_ords.len(), 3, "expected 3 distinct lines");

        assert_glyph_order!(
            glyphs,
            "Autumn moonlight, a worm digs silently into the chestnut."
        );

        let line_2_glyph = &glyphs[18].glyph;
        let line_3_glyph = &&glyphs[39].glyph;
        assert_eq!(line_2_glyph.id, A_FONT.glyph_id('a'));
        assert!(line_2_glyph.position.y > glyphs[0].glyph.position.y);

        assert_eq!(line_3_glyph.id, A_FONT.glyph_id('i'));
        assert!(line_3_glyph.position.y > line_2_glyph.position.y);
    }

    #[test]
    fn single_line_multibyte_chars_finish_at_break() {
        let unicode_str = "❤❤é❤❤\n❤ß❤";
        assert_eq!(
            unicode_str, "\u{2764}\u{2764}\u{e9}\u{2764}\u{2764}\n\u{2764}\u{df}\u{2764}",
            "invisible char funny business",
        );
        assert_eq!(unicode_str.len(), 23);
        assert_eq!(
            xi_unicode::LineBreakIterator::new(unicode_str).find(|n| n.1),
            Some((15, true)),
        );

        let glyphs = Layout::default_single_line().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: unicode_str,
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(glyphs, "\u{2764}\u{2764}\u{e9}\u{2764}\u{2764}");
        assert_relative_eq!(glyphs[0].glyph.position.x, 0.0);
        assert!(
            glyphs[4].glyph.position.x > 0.0,
            "unexpected last position {:?}",
            glyphs[4].glyph.position
        );
    }

    #[test]
    fn no_inherent_section_break() {
        let glyphs = Layout::default_wrap().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (50.0, f32::INFINITY),
                ..SectionGeometry::default()
            },
            &[
                SectionText {
                    text: "The ",
                    ..SectionText::default()
                },
                SectionText {
                    text: "moon",
                    ..SectionText::default()
                },
                SectionText {
                    text: "light",
                    ..SectionText::default()
                },
            ],
        );

        assert_glyph_order!(glyphs, "The moonlight");

        let y_ords: HashSet<OrderedFloat<f32>> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();

        assert_eq!(y_ords.len(), 2, "Y ords: {y_ords:?}");

        let first_line_y = y_ords.iter().min().unwrap();
        let second_line_y = y_ords.iter().max().unwrap();

        assert_relative_eq!(glyphs[0].glyph.position.y, first_line_y);
        assert_relative_eq!(glyphs[4].glyph.position.y, second_line_y);
    }

    #[test]
    fn recalculate_identical() {
        let glyphs = Layout::default().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        let recalc = Layout::default().recalculate_glyphs(
            glyphs,
            GlyphChange::Unknown,
            &*FONT_MAP,
            &SectionGeometry::default(),
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(recalc, "hello world");

        assert_relative_eq!(recalc[0].glyph.position.x, 0.0);
        let last_glyph = &recalc.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );
    }

    #[test]
    fn recalculate_position() {
        let geometry_1 = SectionGeometry {
            screen_position: (0.0, 0.0),
            ..<_>::default()
        };

        let glyphs = Layout::default().calculate_glyphs(
            &*FONT_MAP,
            &geometry_1,
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(20.0),
                font_id: FontId(0),
            }],
        );

        let original_y = glyphs[0].glyph.position.y;

        let recalc = Layout::default().recalculate_glyphs(
            glyphs,
            GlyphChange::Geometry(geometry_1),
            &*FONT_MAP,
            &SectionGeometry {
                screen_position: (0.0, 50.0),
                ..geometry_1
            },
            &[SectionText {
                text: "hello world",
                scale: PxScale::from(20.0),
                ..SectionText::default()
            }],
        );

        assert_glyph_order!(recalc, "hello world");

        assert_relative_eq!(recalc[0].glyph.position.x, 0.0);
        assert_relative_eq!(recalc[0].glyph.position.y, original_y + 50.0);
        let last_glyph = &recalc.last().unwrap().glyph;
        assert!(
            last_glyph.position.x > 0.0,
            "unexpected last position {:?}",
            last_glyph.position
        );
    }

    /// Chinese sentence squeezed into a vertical pipe meaning each character is on
    /// a separate line.
    #[test]
    fn wrap_word_chinese() {
        let glyphs = Layout::default().calculate_glyphs(
            &*FONT_MAP,
            &SectionGeometry {
                bounds: (25.0, f32::INFINITY),
                ..<_>::default()
            },
            &[SectionText {
                text: "提高代碼執行率",
                scale: PxScale::from(20.0),
                font_id: FontId(1),
            }],
        );

        assert_glyph_order!(glyphs, "提高代碼執行率", font = &*CJK_FONT);

        let x_positions: HashSet<_> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.x))
            .collect();
        assert_eq!(x_positions, std::iter::once(OrderedFloat(0.0)).collect());

        let y_positions: HashSet<_> = glyphs
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();

        assert_eq!(y_positions.len(), 7, "{y_positions:?}");
    }

    /// #130 - Respect trailing whitespace in words if directly preceding a hard break.
    /// So right-aligned wrapped on 2 lines `Foo bar` will look different to `Foo \nbar`.
    #[test]
    fn include_spaces_in_layout_width_preceeded_hard_break() {
        // should wrap due to width bound
        let glyphs_no_newline = Layout::default()
            .h_align(HorizontalAlign::Right)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry {
                    bounds: (50.0, f32::INFINITY),
                    ..<_>::default()
                },
                &[SectionText {
                    text: "Foo bar",
                    ..<_>::default()
                }],
            );

        let y_positions: HashSet<_> = glyphs_no_newline
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();
        assert_eq!(y_positions.len(), 2, "{y_positions:?}");

        // explicit wrap
        let glyphs_newline = Layout::default()
            .h_align(HorizontalAlign::Right)
            .calculate_glyphs(
                &*FONT_MAP,
                &SectionGeometry {
                    bounds: (50.0, f32::INFINITY),
                    ..<_>::default()
                },
                &[SectionText {
                    text: "Foo \nbar",
                    ..<_>::default()
                }],
            );

        let y_positions: HashSet<_> = glyphs_newline
            .iter()
            .map(|g| OrderedFloat(g.glyph.position.y))
            .collect();
        assert_eq!(y_positions.len(), 2, "{y_positions:?}");

        // explicit wrap should include the space in the layout width,
        // so the explicit newline `F` should be to the left of the no_newline `F`.
        let newline_f = &glyphs_newline[0];
        let no_newline_f = &glyphs_no_newline[0];
        assert!(
            newline_f.glyph.position.x < no_newline_f.glyph.position.x,
            "explicit newline `F` ({}) should be 1 space to the left of no-newline `F` ({})",
            newline_f.glyph.position.x,
            no_newline_f.glyph.position.x,
        );
    }

    /// #130 - Respect trailing whitespace in words if directly preceding end-of-glyphs.
    /// So right-aligned `Foo ` will look different to `Foo`.
    #[test]
    fn include_spaces_in_layout_width_preceeded_end() {
        let glyphs_no_newline = Layout::default()
            .h_align(HorizontalAlign::Right)
            .calculate_glyphs(
                &*FONT_MAP,
                &<_>::default(),
                &[SectionText {
                    text: "Foo",
                    ..<_>::default()
                }],
            );

        let glyphs_space = Layout::default()
            .h_align(HorizontalAlign::Right)
            .calculate_glyphs(
                &*FONT_MAP,
                &<_>::default(),
                &[SectionText {
                    text: "Foo   ",
                    ..<_>::default()
                }],
            );

        let space_f = &glyphs_space[0];
        let no_space_f = &glyphs_no_newline[0];
        assert!(
            space_f.glyph.position.x < no_space_f.glyph.position.x,
            "with-space `F` ({}) should be 3 spaces to the left of no-space `F` ({})",
            space_f.glyph.position.x,
            no_space_f.glyph.position.x,
        );
    }
}
