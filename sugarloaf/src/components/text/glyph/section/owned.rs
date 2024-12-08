// glyph module code along with comments was originally retired from glyph-brush
// https://github.com/alexheretic/glyph-brush
// glyph-brush was originally written Alex Butler (https://github.com/alexheretic)
// and licensed under Apache-2.0 license.

use crate::components::text::glyph::extra::Color;
use crate::components::text::glyph::extra::Extra;
use crate::components::text::glyph::layout::Layout;
use crate::components::text::glyph::section::Section;
use crate::components::text::glyph::section::Text;
use crate::components::text::glyph::BuiltInLineBreaker;
use crate::components::text::glyph::FontId;
use ab_glyph::PxScale;
use std::{borrow::Cow, f32};

#[derive(Debug, Clone, PartialEq)]
pub struct OwnedSection<X = Extra> {
    /// Position on screen to render text, in pixels from top-left. Defaults to (0, 0).
    pub screen_position: (f32, f32),
    /// Max (width, height) bounds, in pixels from top-left. Defaults to unbounded.
    pub bounds: (f32, f32),
    /// Built in layout, can be overridden with custom layout logic
    /// see [`queue_custom_layout`](struct.GlyphBrush.html#method.queue_custom_layout)
    pub layout: Layout<BuiltInLineBreaker>,
    /// Text to render, rendered next to one another according the layout.
    pub text: Vec<OwnedText<X>>,
}

impl<X: Default> Default for OwnedSection<X> {
    #[inline]
    fn default() -> Self {
        Self {
            screen_position: (0.0, 0.0),
            bounds: (f32::INFINITY, f32::INFINITY),
            layout: Layout::default(),
            text: vec![],
        }
    }
}

impl<X> OwnedSection<X> {
    #[inline]
    pub fn with_screen_position<P: Into<(f32, f32)>>(mut self, position: P) -> Self {
        self.screen_position = position.into();
        self
    }

    #[inline]
    pub fn with_bounds<P: Into<(f32, f32)>>(mut self, bounds: P) -> Self {
        self.bounds = bounds.into();
        self
    }

    #[inline]
    pub fn with_layout<L: Into<Layout<BuiltInLineBreaker>>>(mut self, layout: L) -> Self {
        self.layout = layout.into();
        self
    }

    #[inline]
    pub fn add_text<T: Into<OwnedText<X>>>(mut self, text: T) -> Self {
        self.text.push(text.into());
        self
    }

    #[inline]
    pub fn with_text<X2>(self, text: Vec<OwnedText<X2>>) -> OwnedSection<X2> {
        OwnedSection {
            text,
            screen_position: self.screen_position,
            bounds: self.bounds,
            layout: self.layout,
        }
    }
}

impl<X: Clone> OwnedSection<X> {
    pub fn to_borrowed(&self) -> Section<'_, X> {
        Section {
            screen_position: self.screen_position,
            bounds: self.bounds,
            layout: self.layout,
            text: self.text.iter().map(|t| t.into()).collect(),
        }
    }
}

impl<'a> From<&'a OwnedSection> for Section<'a> {
    fn from(owned: &'a OwnedSection) -> Self {
        owned.to_borrowed()
    }
}

impl<'a> From<&'a OwnedSection> for Cow<'a, Section<'a>> {
    fn from(owned: &'a OwnedSection) -> Self {
        Cow::Owned(owned.to_borrowed())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnedText<X = Extra> {
    /// Text to render.
    pub text: String,
    /// Pixel scale of text. Defaults to 16.
    pub scale: PxScale,
    /// Font id to use for this section.
    ///
    /// It must be known to the `GlyphBrush` it is being used with,
    /// either `FontId::default()` or the return of
    /// [`add_font`](struct.GlyphBrushBuilder.html#method.add_font).
    pub font_id: FontId,
    // Extra stuff for vertex generation.
    pub extra: X,
}

impl<X> OwnedText<X> {
    #[inline]
    pub fn with_text<T: Into<String>>(mut self, text: T) -> Self {
        self.text = text.into();
        self
    }

    #[inline]
    pub fn with_scale<S: Into<PxScale>>(mut self, scale: S) -> Self {
        self.scale = scale.into();
        self
    }

    #[inline]
    pub fn with_font_id<F: Into<FontId>>(mut self, font_id: F) -> Self {
        self.font_id = font_id.into();
        self
    }

    #[inline]
    pub fn with_extra<X2>(self, extra: X2) -> OwnedText<X2> {
        OwnedText {
            text: self.text,
            scale: self.scale,
            font_id: self.font_id,
            extra,
        }
    }
}

impl OwnedText<Extra> {
    #[inline]
    pub fn new<T: Into<String>>(text: T) -> Self {
        OwnedText::default().with_text(text)
    }

    #[inline]
    pub fn with_color<C: Into<Color>>(mut self, color: C) -> Self {
        self.extra.color = color.into();
        self
    }

    #[inline]
    pub fn with_z<Z: Into<f32>>(mut self, z: Z) -> Self {
        self.extra.z = z.into();
        self
    }
}

impl<X: Default> Default for OwnedText<X> {
    #[inline]
    fn default() -> Self {
        Self {
            text: String::new(),
            scale: PxScale::from(16.0),
            font_id: <_>::default(),
            extra: <_>::default(),
        }
    }
}

impl<'a, X: Clone> From<&'a OwnedText<X>> for Text<'a, X> {
    #[inline]
    fn from(owned: &'a OwnedText<X>) -> Self {
        Self {
            text: owned.text.as_str(),
            scale: owned.scale,
            font_id: owned.font_id,
            extra: owned.extra.clone(),
        }
    }
}

impl<X: Clone> From<&Text<'_, X>> for OwnedText<X> {
    #[inline]
    fn from(s: &Text<'_, X>) -> Self {
        Self {
            text: s.text.into(),
            scale: s.scale,
            font_id: s.font_id,
            extra: s.extra.clone(),
        }
    }
}
