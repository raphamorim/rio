pub use super::font::FamilyList;
use crate::core::SugarCursor;
pub use swash::text::Language;
use swash::{Setting, Stretch, Style, Weight};

use std::borrow::Cow;

/// Style that can be applied to a range of text.
#[derive(Debug, Clone)]
pub enum SpanStyle<'a> {
    /// Prioritized list of font families.
    FamilyList(FamilyList),
    /// Font size.
    Size(f32),
    /// Font stretch.
    Stretch(Stretch),
    /// Font weight.
    Weight(Weight),
    /// Font color.
    Color([f32; 4]),
    /// Background color.
    BackgroundColor([f32; 4]),
    /// Font style.
    Style(Style),
    /// Cursor.
    Cursor(SugarCursor),
    /// Font feature settings.
    Features(Cow<'a, [Setting<u16>]>),
    /// Font variation settings.
    Variations(Cow<'a, [Setting<f32>]>),
    /// Text direction.
    Direction(Direction),
    /// Text language.
    Language(Language),
    /// Additional spacing between letters.
    LetterSpacing(f32),
    /// Additional spacing between words.
    WordSpacing(f32),
    /// Multiplicative line spacing factor.
    LineSpacing(f32),
    /// Underline decoration.
    Underline(bool),
    /// Underline color.
    UnderlineColor([f32; 4]),
    /// Offset of an underline. Set to `None` to use the font value.
    UnderlineOffset(Option<f32>),
    /// Thickness of an underline. Set to `None` to use the font value.
    UnderlineSize(Option<f32>),
    /// Text case transform.
    TextTransform(TextTransform),
}

impl<'a> SpanStyle<'a> {
    pub fn family_list(families: impl Into<FamilyList>) -> Self {
        Self::FamilyList(families.into())
    }

    pub fn features(features: impl Into<Cow<'a, [Setting<u16>]>>) -> Self {
        Self::Features(features.into())
    }

    pub fn variations(variations: impl Into<Cow<'a, [Setting<f32>]>>) -> Self {
        Self::Variations(variations.into())
    }

    pub fn to_owned(&self) -> SpanStyle<'static> {
        use SpanStyle as S;
        match self {
            Self::FamilyList(v) => S::FamilyList(v.clone()),
            Self::Size(v) => S::Size(*v),
            Self::Stretch(v) => S::Stretch(*v),
            Self::Weight(v) => S::Weight(*v),
            Self::Color(v) => S::Color(*v),
            Self::BackgroundColor(v) => S::BackgroundColor(*v),
            Self::Style(v) => S::Style(*v),
            Self::Features(v) => S::Features(Cow::Owned(v.clone().into_owned())),
            Self::Variations(v) => S::Variations(Cow::Owned(v.clone().into_owned())),
            Self::Direction(v) => S::Direction(*v),
            Self::Language(v) => S::Language(*v),
            Self::LetterSpacing(v) => S::LetterSpacing(*v),
            Self::WordSpacing(v) => S::WordSpacing(*v),
            Self::LineSpacing(v) => S::LineSpacing(*v),
            Self::Underline(v) => S::Underline(*v),
            Self::UnderlineOffset(v) => S::UnderlineOffset(*v),
            Self::UnderlineColor(v) => S::UnderlineColor(*v),
            Self::UnderlineSize(v) => S::UnderlineSize(*v),
            Self::TextTransform(v) => S::TextTransform(*v),
            Self::Cursor(v) => S::Cursor(*v),
        }
    }

    pub fn into_owned(self) -> SpanStyle<'static> {
        use SpanStyle as S;
        match self {
            Self::FamilyList(v) => S::FamilyList(v.clone()),
            Self::Size(v) => S::Size(v),
            Self::Stretch(v) => S::Stretch(v),
            Self::Weight(v) => S::Weight(v),
            Self::Color(v) => S::Color(v),
            Self::BackgroundColor(v) => S::BackgroundColor(v),
            Self::Style(v) => S::Style(v),
            Self::Features(v) => S::Features(Cow::Owned(v.into_owned())),
            Self::Variations(v) => S::Variations(Cow::Owned(v.into_owned())),
            Self::Direction(v) => S::Direction(v),
            Self::Language(v) => S::Language(v),
            Self::LetterSpacing(v) => S::LetterSpacing(v),
            Self::WordSpacing(v) => S::WordSpacing(v),
            Self::LineSpacing(v) => S::LineSpacing(v),
            Self::Underline(v) => S::Underline(v),
            Self::UnderlineOffset(v) => S::UnderlineOffset(v),
            Self::UnderlineColor(v) => S::UnderlineColor(v),
            Self::UnderlineSize(v) => S::UnderlineSize(v),
            Self::TextTransform(v) => S::TextTransform(v),
            Self::Cursor(v) => S::Cursor(v),
        }
    }

    pub fn same_kind(&self, other: &SpanStyle) -> bool {
        use core::mem::discriminant;
        discriminant(self) == discriminant(other)
    }
}

/// Paragraph direction.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Direction {
    Auto,
    LeftToRight,
    RightToLeft,
}

impl Default for Direction {
    fn default() -> Self {
        Self::LeftToRight
    }
}

impl From<Direction> for SpanStyle<'static> {
    fn from(value: Direction) -> Self {
        Self::Direction(value)
    }
}

/// Specifies a case transformation for text.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TextTransform {
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

impl From<TextTransform> for SpanStyle<'static> {
    fn from(value: TextTransform) -> Self {
        Self::TextTransform(value)
    }
}

impl From<&str> for SpanStyle<'static> {
    fn from(s: &str) -> Self {
        Self::family_list(s)
    }
}
