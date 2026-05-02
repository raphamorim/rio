// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Span-level styling: the type set that survived the rich-text
//! emission deletion. `SpanStyle` is shared across grid emit (via
//! `font_cache::FontCache::find_best_font_match`), UI text shaping
//! (`crate::text`), and rio's frontend (font_cache + grid_emit). The
//! decoration enums are read by the underline / strikethrough quad
//! batches in the renderer.

use crate::sugarloaf::primitives::SugarCursor;
use crate::{DrawableChar, Graphic};
use swash::{Attributes, Setting};

/// Index into a font setting cache.
pub type FontSettingKey = u32;

/// Cache of tag/value pairs for font settings.
#[derive(Default, Clone, Debug)]
pub struct FontSettingCache<T: Copy + PartialOrd + PartialEq + std::fmt::Debug> {
    settings: Vec<Setting<T>>,
    lists: Vec<FontSettingList>,
    tmp: Vec<Setting<T>>,
}

impl<T: Copy + PartialOrd + PartialEq + std::fmt::Debug> FontSettingCache<T> {
    pub fn get(&self, key: u32) -> &[Setting<T>] {
        if key == !0 {
            &[]
        } else {
            self.lists
                .get(key as usize)
                .map(|list| list.get(&self.settings))
                .unwrap_or(&[])
        }
    }

    pub fn clear(&mut self) {
        self.settings.clear();
        self.lists.clear();
        self.tmp.clear();
    }
}

/// Sentinel for an empty set of font settings.
pub const EMPTY_FONT_SETTINGS: FontSettingKey = !0;

/// Range within a font setting cache.
#[derive(Copy, Clone, Debug)]
struct FontSettingList {
    pub start: u32,
    pub end: u32,
}

impl FontSettingList {
    pub fn get<T>(self, elements: &[T]) -> &[T] {
        elements
            .get(self.start as usize..self.end as usize)
            .unwrap_or(&[])
    }
}

#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub enum UnderlineShape {
    #[default]
    Regular = 0,
    Dotted = 1,
    Dashed = 2,
    Curly = 3,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct UnderlineInfo {
    pub is_doubled: bool,
    pub shape: UnderlineShape,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SpanStyleDecoration {
    Underline(UnderlineInfo),
    Strikethrough,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SpanStyle {
    pub font_id: usize,
    /// Unicode width.
    pub width: f32,
    /// Font attributes.
    pub font_attrs: Attributes,
    /// Font color.
    pub color: [f32; 4],
    /// Background color.
    pub background_color: Option<[f32; 4]>,
    /// Font variations.
    pub font_vars: FontSettingKey,
    /// Enable underline / strikethrough decoration.
    pub decoration: Option<SpanStyleDecoration>,
    /// Decoration color.
    pub decoration_color: Option<[f32; 4]>,
    /// Cursor style.
    pub cursor: Option<SugarCursor>,
    /// Media (kitty-protocol image).
    pub media: Option<Graphic>,
    /// Drawable character (Unicode box-drawing / Powerline / sextants).
    pub drawable_char: Option<DrawableChar>,
    /// PUA constraint width: how many cells the glyph should visually
    /// fill. None for normal glyphs, Some(1.0) or Some(2.0) for PUA
    /// glyphs. Does NOT affect positioning/advance — only compositor
    /// scaling.
    pub pua_constraint: Option<f32>,
    /// Optional per-glyph Nerd Font constraint (size / alignment /
    /// padding) sourced from the Nerd Fonts patcher table. When set,
    /// the compositor lays the glyph out using the constraint math
    /// in `nerd_font_attributes` instead of the generic cell-centered
    /// fit. Only populated by the renderer for codepoints with a
    /// table entry (`get_constraint`).
    pub nerd_font_constraint: Option<crate::font::nerd_font_attributes::Constraint>,
}

impl Default for SpanStyle {
    fn default() -> Self {
        Self {
            font_id: 0,
            width: 1.0,
            font_attrs: Attributes::default(),
            font_vars: EMPTY_FONT_SETTINGS,
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            cursor: None,
            decoration: None,
            decoration_color: None,
            media: None,
            drawable_char: None,
            pua_constraint: None,
            nerd_font_constraint: None,
        }
    }
}
