// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use crate::glyph::BuiltInLineBreaker;
use crate::glyph::Color;
use crate::glyph::FontId;
use crate::glyph::Layout;
use crate::glyph::SectionGeometry;
use crate::glyph::Text;
use ab_glyph::PxScale;
use ordered_float::OrderedFloat;
use std::{borrow::Cow, f32, hash::*};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SectionText<'a> {
    /// Text to render
    pub text: &'a str,
    /// Pixel scale of text. Defaults to 16.
    pub scale: PxScale,
    /// Rgba color of rendered text. Defaults to black.
    pub color: Color,
    /// Font id to use for this section.
    ///
    /// It must be known to the `GlyphBrush` it is being used with,
    /// either `FontId::default()` or the return of
    pub font_id: FontId,
}

impl Default for SectionText<'static> {
    #[inline]
    fn default() -> Self {
        Self {
            text: "",
            scale: PxScale::from(16.0),
            color: [0.0, 0.0, 0.0, 1.0],
            font_id: <_>::default(),
        }
    }
}

impl<'a> SectionText<'a> {
    #[allow(clippy::wrong_self_convention)] // won't fix for backward compatibility
    #[inline]
    pub fn to_text(&self, z: f32) -> crate::glyph::Text<'a> {
        crate::glyph::Text::new(self.text)
            .with_scale(self.scale)
            .with_color(self.color)
            .with_font_id(self.font_id)
            .with_z(z)
    }
}

impl<'a> From<&crate::glyph::Text<'a>> for SectionText<'a> {
    #[inline]
    fn from(t: &crate::glyph::Text<'a>) -> Self {
        Self {
            text: t.text,
            scale: t.scale,
            color: t.extra.color,
            font_id: t.font_id,
        }
    }
}

impl<'a> From<crate::glyph::Text<'a>> for SectionText<'a> {
    #[inline]
    fn from(t: crate::glyph::Text<'a>) -> Self {
        Self::from(&t)
    }
}

/// An object that contains all the info to render a varied section of text. That is one including
/// many parts with differing fonts/scales/colors bowing to a single layout.
///
/// For single font/scale/color sections it may be simpler to use
/// [`Section`](struct.Section.html).
#[derive(Debug, Clone, PartialEq)]
pub struct VariedSection<'a> {
    /// Position on screen to render text, in pixels from top-left. Defaults to (0, 0).
    pub screen_position: (f32, f32),
    /// Max (width, height) bounds, in pixels from top-left. Defaults to unbounded.
    pub bounds: (f32, f32),
    /// Z values for use in depth testing. Defaults to 0.0
    pub z: f32,
    /// Built in layout, can be overridden with custom layout logic
    /// see [`queue_custom_layout`](struct.GlyphBrush.html#method.queue_custom_layout)
    pub layout: Layout<BuiltInLineBreaker>,
    /// Text to render, rendered next to one another according the layout.
    pub text: Vec<SectionText<'a>>,
}

impl Default for VariedSection<'static> {
    #[inline]
    fn default() -> Self {
        Self {
            screen_position: (0.0, 0.0),
            bounds: (f32::INFINITY, f32::INFINITY),
            z: 0.0,
            layout: Layout::default(),
            text: vec![],
        }
    }
}

impl<'a> From<VariedSection<'a>> for Cow<'a, VariedSection<'a>> {
    fn from(owned: VariedSection<'a>) -> Self {
        Cow::Owned(owned)
    }
}

impl<'a, 'b> From<&'b VariedSection<'a>> for Cow<'b, VariedSection<'a>> {
    fn from(owned: &'b VariedSection<'a>) -> Self {
        Cow::Borrowed(owned)
    }
}

impl<'a> From<VariedSection<'a>> for Cow<'a, crate::glyph::Section<'a>> {
    #[inline]
    fn from(s: VariedSection<'a>) -> Self {
        Cow::Owned(s.into())
    }
}

impl<'a, 'b> From<&'b VariedSection<'a>> for Cow<'b, crate::glyph::Section<'a>> {
    #[inline]
    fn from(s: &'b VariedSection<'a>) -> Self {
        Cow::Owned(s.into())
    }
}

impl<'a> From<&crate::glyph::Section<'a>> for VariedSection<'a> {
    #[inline]
    fn from(s: &crate::glyph::Section<'a>) -> Self {
        Self {
            text: s.text.iter().map(SectionText::from).collect(),
            bounds: s.bounds,
            screen_position: s.screen_position,
            layout: s.layout,
            // take the first z value, good enough for legacy compatibility
            z: s.text.get(0).map(|t| t.extra.z).unwrap_or(0.0),
        }
    }
}

impl Hash for VariedSection<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let VariedSection {
            screen_position: (screen_x, screen_y),
            bounds: (bound_w, bound_h),
            z,
            layout,
            ref text,
        } = *self;

        let ord_floats: &[OrderedFloat<_>] = &[
            screen_x.into(),
            screen_y.into(),
            bound_w.into(),
            bound_h.into(),
            z.into(),
        ];

        layout.hash(state);

        hash_section_text(state, text);

        ord_floats.hash(state);
    }
}

#[inline]
fn hash_section_text<H: Hasher>(state: &mut H, text: &[SectionText]) {
    for t in text {
        let SectionText {
            text,
            scale,
            color,
            font_id,
        } = *t;

        let ord_floats: &[OrderedFloat<_>] = &[
            scale.x.into(),
            scale.y.into(),
            color[0].into(),
            color[1].into(),
            color[2].into(),
            color[3].into(),
        ];

        (text, font_id, ord_floats).hash(state);
    }
}

impl<'text> VariedSection<'text> {
    pub fn to_owned(&self) -> OwnedVariedSection {
        OwnedVariedSection {
            screen_position: self.screen_position,
            bounds: self.bounds,
            z: self.z,
            layout: self.layout,
            text: self.text.iter().map(OwnedSectionText::from).collect(),
        }
    }
}

impl From<&VariedSection<'_>> for SectionGeometry {
    fn from(section: &VariedSection<'_>) -> Self {
        Self {
            bounds: section.bounds,
            screen_position: section.screen_position,
        }
    }
}

impl<'a> From<&VariedSection<'a>> for crate::glyph::Section<'a> {
    #[inline]
    fn from(s: &VariedSection<'a>) -> Self {
        crate::glyph::Section::builder()
            .with_layout(s.layout)
            .with_bounds(s.bounds)
            .with_screen_position(s.screen_position)
            .with_text(s.text.iter().map(|t| t.to_text(s.z)).collect())
    }
}

impl<'a> From<VariedSection<'a>> for crate::glyph::Section<'a> {
    #[inline]
    fn from(s: VariedSection<'a>) -> Self {
        Self::from(&s)
    }
}

/// An object that contains all the info to render a section of text.
///
/// For varied font/scale/color sections see [`VariedSection`](struct.VariedSection.html).
///
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Section<'a> {
    /// Text to render
    pub text: &'a str,
    /// Position on screen to render text, in pixels from top-left. Defaults to (0, 0).
    pub screen_position: (f32, f32),
    /// Max (width, height) bounds, in pixels from top-left. Defaults to unbounded.
    pub bounds: (f32, f32),
    /// Pixel scale of text. Defaults to 16.
    pub scale: PxScale,
    /// Rgba color of rendered text. Defaults to black.
    pub color: [f32; 4],
    /// Z values for use in depth testing. Defaults to 0.0
    pub z: f32,
    /// Built in layout, can overridden with custom layout logic
    /// see [`queue_custom_layout`](struct.GlyphBrush.html#method.queue_custom_layout)
    pub layout: Layout<BuiltInLineBreaker>,
    /// Font id to use for this section.
    ///
    /// It must be known to the `GlyphBrush` it is being used with,
    /// either `FontId::default()` or the return of
    /// `add_font`
    pub font_id: FontId,
}

impl Default for Section<'static> {
    #[inline]
    fn default() -> Self {
        Self {
            text: "",
            screen_position: (0.0, 0.0),
            bounds: (f32::INFINITY, f32::INFINITY),
            scale: PxScale::from(16.0),
            color: [0.0, 0.0, 0.0, 1.0],
            z: 0.0,
            layout: Layout::default(),
            font_id: FontId::default(),
        }
    }
}

impl<'a> From<&Section<'a>> for VariedSection<'a> {
    fn from(s: &Section<'a>) -> Self {
        let Section {
            text,
            scale,
            color,
            screen_position,
            bounds,
            z,
            layout,
            font_id,
        } = *s;

        VariedSection {
            text: vec![SectionText {
                text,
                scale,
                color,
                font_id,
            }],
            screen_position,
            bounds,
            z,
            layout,
        }
    }
}

impl<'a> From<Section<'a>> for VariedSection<'a> {
    fn from(s: Section<'a>) -> Self {
        VariedSection::from(&s)
    }
}

impl<'a> From<Section<'a>> for Cow<'a, VariedSection<'a>> {
    fn from(section: Section<'a>) -> Self {
        Cow::Owned(VariedSection::from(section))
    }
}

impl<'a> From<&Section<'a>> for Cow<'a, VariedSection<'a>> {
    fn from(section: &Section<'a>) -> Self {
        Cow::Owned(VariedSection::from(section))
    }
}

impl<'a> From<&Section<'a>> for crate::glyph::Section<'a> {
    fn from(s: &Section<'a>) -> Self {
        let Section {
            text,
            scale,
            color,
            screen_position,
            bounds,
            z,
            layout,
            font_id,
        } = *s;

        crate::glyph::Section::default()
            .add_text(
                Text::new(text)
                    .with_scale(scale)
                    .with_color(color)
                    .with_z(z)
                    .with_font_id(font_id),
            )
            .with_screen_position(screen_position)
            .with_bounds(bounds)
            .with_layout(layout)
    }
}

impl<'a> From<Section<'a>> for crate::glyph::Section<'a> {
    fn from(s: Section<'a>) -> Self {
        crate::glyph::Section::from(&s)
    }
}

impl<'a> From<Section<'a>> for Cow<'a, crate::glyph::Section<'a>> {
    fn from(section: Section<'a>) -> Self {
        Cow::Owned(crate::glyph::Section::from(section))
    }
}

impl<'a> From<&Section<'a>> for Cow<'a, crate::glyph::Section<'a>> {
    fn from(section: &Section<'a>) -> Self {
        Cow::Owned(crate::glyph::Section::from(section))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnedVariedSection {
    /// Position on screen to render text, in pixels from top-left. Defaults to (0, 0).
    pub screen_position: (f32, f32),
    /// Max (width, height) bounds, in pixels from top-left. Defaults to unbounded.
    pub bounds: (f32, f32),
    /// Z values for use in depth testing. Defaults to 0.0
    pub z: f32,
    /// Built in layout, can be overridden with custom layout logic
    /// see [`queue_custom_layout`](struct.GlyphBrush.html#method.queue_custom_layout)
    pub layout: Layout<BuiltInLineBreaker>,
    /// Text to render, rendered next to one another according the layout.
    pub text: Vec<OwnedSectionText>,
}

impl Default for OwnedVariedSection {
    fn default() -> Self {
        Self {
            screen_position: (0.0, 0.0),
            bounds: (f32::INFINITY, f32::INFINITY),
            z: 0.0,
            layout: Layout::default(),
            text: vec![],
        }
    }
}

impl OwnedVariedSection {
    #[inline]
    pub fn to_borrowed(&self) -> VariedSection<'_> {
        VariedSection {
            screen_position: self.screen_position,
            bounds: self.bounds,
            z: self.z,
            layout: self.layout,
            text: self.text.iter().map(|t| t.into()).collect(),
        }
    }
}

impl<'a> From<&'a OwnedVariedSection> for VariedSection<'a> {
    fn from(owned: &'a OwnedVariedSection) -> Self {
        owned.to_borrowed()
    }
}

impl<'a> From<&'a OwnedVariedSection> for Cow<'a, VariedSection<'a>> {
    fn from(owned: &'a OwnedVariedSection) -> Self {
        Cow::Owned(owned.to_borrowed())
    }
}

impl<'a> From<&'a OwnedVariedSection> for crate::glyph::Section<'a> {
    #[inline]
    fn from(owned: &'a OwnedVariedSection) -> Self {
        owned.to_borrowed().into()
    }
}

impl<'a> From<&'a OwnedVariedSection> for Cow<'a, crate::glyph::Section<'a>> {
    #[inline]
    fn from(owned: &'a OwnedVariedSection) -> Self {
        Cow::Owned(owned.to_borrowed().into())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnedSectionText {
    /// Text to render
    pub text: String,
    /// Pixel scale of text. Defaults to 16.
    pub scale: PxScale,
    /// Rgba color of rendered text. Defaults to black.
    pub color: [f32; 4],
    /// Font id to use for this section.
    ///
    /// It must be known to the `GlyphBrush` it is being used with,
    /// either `FontId::default()` or the return of
    /// [`add_font`](struct.GlyphBrushBuilder.html#method.add_font).
    pub font_id: FontId,
}

impl Default for OwnedSectionText {
    fn default() -> Self {
        Self {
            text: String::new(),
            scale: PxScale::from(16.0),
            color: [0.0, 0.0, 0.0, 1.0],
            font_id: FontId::default(),
        }
    }
}

impl<'a> From<&'a OwnedSectionText> for SectionText<'a> {
    fn from(owned: &'a OwnedSectionText) -> Self {
        Self {
            text: owned.text.as_str(),
            scale: owned.scale,
            color: owned.color,
            font_id: owned.font_id,
        }
    }
}

impl From<&SectionText<'_>> for OwnedSectionText {
    fn from(st: &SectionText<'_>) -> Self {
        Self {
            text: st.text.into(),
            scale: st.scale,
            color: st.color,
            font_id: st.font_id,
        }
    }
}
