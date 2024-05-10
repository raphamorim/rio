// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// span_style.rs suffered alterations but was originally retired from dfrg/swash_demo
// which is licensed under MIT https://github.com/dfrg/swash_demo/blob/master/LICENSE
//
// This file however suffered updates made by Raphael Amorim to support
// underline_color, background_color, text color and other functionalities

use crate::layout::builder_data::FontSettingKey;
use crate::layout::builder_data::EMPTY_FONT_SETTINGS;
use crate::sugarloaf::primitives::SugarCursor;
use crate::Sugar;
use crate::SugarDecoration;
use crate::SugarStyle;
pub use swash::text::Language;
use swash::{Setting, Stretch, Style, Weight};

use std::borrow::Cow;

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct FragmentStyle {
    // Text direction.
    pub dir: Direction,
    // Is the direction different from the parent?
    pub dir_changed: bool,
    /// Text language.
    pub lang: Option<Language>,
    /// Internal identifier for a list of font families and attributes.
    pub font: usize,
    /// Font attributes.
    pub font_attrs: (Stretch, Weight, Style),
    /// Font size in ppem.
    pub font_size: f32,
    /// Font color.
    pub color: [f32; 4],
    /// Background color.
    pub background_color: Option<[f32; 4]>,
    /// Font features.
    pub font_features: FontSettingKey,
    /// Font variations.
    pub font_vars: FontSettingKey,
    /// Additional spacing between letters (clusters) of text.
    pub letter_spacing: f32,
    /// Additional spacing between words of text.
    pub word_spacing: f32,
    /// Multiplicative line spacing factor.
    pub line_spacing: f32,
    /// Enable underline decoration.
    pub underline: bool,
    /// Offset of an underline.
    pub underline_offset: Option<f32>,
    /// Color of an underline.
    pub underline_color: Option<[f32; 4]>,
    /// Thickness of an underline.
    pub underline_size: Option<f32>,
    /// Text case transformation.
    pub text_transform: TextTransform,
    /// Cursor
    pub cursor: SugarCursor,
}

impl Default for FragmentStyle {
    fn default() -> Self {
        Self {
            dir: Direction::LeftToRight,
            dir_changed: false,
            lang: None,
            font: 0,
            font_attrs: (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
            font_size: 16.,
            font_features: EMPTY_FONT_SETTINGS,
            font_vars: EMPTY_FONT_SETTINGS,
            letter_spacing: 0.,
            word_spacing: 0.,
            line_spacing: 1.,
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            cursor: SugarCursor::Disabled,
            underline: false,
            underline_offset: None,
            underline_color: None,
            underline_size: None,
            text_transform: TextTransform::None,
        }
    }
}

impl FragmentStyle {
    pub fn scaled_default(scale: f32) -> Self {
        Self {
            dir: Direction::LeftToRight,
            dir_changed: false,
            lang: None,
            font: 0,
            font_attrs: (Stretch::NORMAL, Weight::NORMAL, Style::Normal),
            font_size: 16. * scale,
            font_features: EMPTY_FONT_SETTINGS,
            font_vars: EMPTY_FONT_SETTINGS,
            letter_spacing: 0.,
            word_spacing: 0.,
            line_spacing: 1.,
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            cursor: SugarCursor::Disabled,
            underline: false,
            underline_offset: None,
            underline_color: None,
            underline_size: None,
            text_transform: TextTransform::None,
        }
    }
}

impl From<&Sugar> for FragmentStyle {
    fn from(sugar: &Sugar) -> Self {
        let mut style = FragmentStyle::default();

        match sugar.style {
            SugarStyle::BoldItalic => {
                style.font_attrs.1 = Weight::BOLD;
                style.font_attrs.2 = Style::Italic;
            }
            SugarStyle::Bold => {
                style.font_attrs.1 = Weight::BOLD;
            }
            SugarStyle::Italic => {
                style.font_attrs.2 = Style::Italic;
            }
            _ => {}
        }

        let mut has_underline_cursor = false;
        match sugar.cursor {
            SugarCursor::Underline(cursor_color) => {
                style.underline = true;
                style.underline_offset = Some(-1.);
                style.underline_color = Some(cursor_color);
                style.underline_size = Some(-1.);

                has_underline_cursor = true;
            }
            SugarCursor::Block(cursor_color) => {
                style.cursor = SugarCursor::Block(cursor_color);
            }
            SugarCursor::Caret(cursor_color) => {
                style.cursor = SugarCursor::Caret(cursor_color);
            }
            _ => {}
        }

        match &sugar.decoration {
            SugarDecoration::Underline => {
                if !has_underline_cursor {
                    style.underline = true;
                    style.underline_offset = Some(-2.);
                    style.underline_size = Some(1.);
                }
            }
            SugarDecoration::Strikethrough => {
                style.underline = true;
                style.underline_offset = Some(style.font_size / 2.);
                style.underline_size = Some(2.);
            }
            _ => {}
        }

        style.color = sugar.foreground_color;
        style.background_color = Some(sugar.background_color);

        style
    }
}

/// Style that can be applied to a range of text.
#[derive(Debug, PartialEq, Clone)]
pub enum SpanStyle {
    /// Font Id
    FontId(usize),
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
    Features(Vec<Setting<u16>>),
    /// Font variation settings.
    Variations(Vec<Setting<f32>>),
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

impl<'a> SpanStyle {
    pub fn features(features: impl Into<Cow<'a, [Setting<u16>]>>) -> Self {
        Self::Features(features.into().to_vec())
    }

    pub fn variations(variations: impl Into<Cow<'a, [Setting<f32>]>>) -> Self {
        Self::Variations(variations.into().to_vec())
    }

    pub fn to_owned(&self) -> SpanStyle {
        use SpanStyle as S;
        match self {
            Self::FontId(v) => S::FontId(*v),
            Self::Size(v) => S::Size(*v),
            Self::Stretch(v) => S::Stretch(*v),
            Self::Weight(v) => S::Weight(*v),
            Self::Color(v) => S::Color(*v),
            Self::BackgroundColor(v) => S::BackgroundColor(*v),
            Self::Style(v) => S::Style(*v),
            Self::Features(v) => S::Features(v.to_owned()),
            Self::Variations(v) => S::Variations(v.to_owned()),
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

    pub fn into_owned(self) -> SpanStyle {
        use SpanStyle as S;
        match self {
            Self::FontId(v) => S::FontId(v),
            Self::Size(v) => S::Size(v),
            Self::Stretch(v) => S::Stretch(v),
            Self::Weight(v) => S::Weight(v),
            Self::Color(v) => S::Color(v),
            Self::BackgroundColor(v) => S::BackgroundColor(v),
            Self::Style(v) => S::Style(v),
            Self::Features(v) => S::Features(v.to_owned()),
            Self::Variations(v) => S::Variations(v.to_owned()),
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

impl From<Direction> for SpanStyle {
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

impl From<TextTransform> for SpanStyle {
    fn from(value: TextTransform) -> Self {
        Self::TextTransform(value)
    }
}
