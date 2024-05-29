// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// layout_data.rs was originally retired from dfrg/swash_demo licensed under MIT
// https://github.com/dfrg/swash_demo/blob/master/LICENSE
//
// This file had updates to support color, underline_color, background_color
// and other functionalities

use super::span_style::*;
use swash::text::{cluster::CharInfo, Script};
use swash::Setting;

/// Data that describes a fragment.
#[derive(Copy, Debug, Clone)]
pub struct FragmentData {
    /// Identifier of the span that contains the fragment.
    pub span: usize,
    /// True if this fragment breaks shaping with the previous fragment.
    pub break_shaping: bool,
    /// Offset of the text.
    pub start: usize,
    /// End of the text.
    pub end: usize,
    /// Internal identifier for a list of font families and attributes.
    pub font: usize,
    /// Font features.
    pub features: FontSettingKey,
    /// Font variations.
    pub vars: FontSettingKey,
}

/// Data that describes an item.
#[derive(Copy, Debug, Clone)]
pub struct ItemData {
    /// Script of the item.
    pub script: Script,
    /// Bidi level of the item.
    pub level: u8,
    /// Offset of the text.
    pub start: usize,
    /// End of the text.
    pub end: usize,
    /// Font features.
    pub features: FontSettingKey,
    /// Font variations.
    pub vars: FontSettingKey,
}

/// Builder Line State
#[derive(Default)]
pub struct BuilderLineText {
    /// Combined text.
    pub content: Vec<char>,
    /// Fragment index per character.
    pub frags: Vec<u32>,
    /// Span index per character.
    pub spans: Vec<usize>,
    /// Character info per character.
    pub info: Vec<CharInfo>,
    /// Offset of each character relative to its fragment.
    pub offsets: Vec<u32>,
}

#[derive(Default)]
pub struct BuilderLine {
    pub text: BuilderLineText,
    /// Collection of fragments.
    pub fragments: Vec<FragmentData>,
    /// Collection of items.
    pub items: Vec<ItemData>,
    /// Span index per character.
    pub styles: Vec<FragmentStyle>,
    /// Line Hash
    pub hash: Option<u64>,
}

/// Builder state.
#[derive(Default)]
pub struct BuilderState {
    /// Lines State
    pub lines: Vec<BuilderLine>,
    /// Font feature setting cache.
    pub features: FontSettingCache<u16>,
    /// Font variation setting cache.
    pub vars: FontSettingCache<f32>,
    /// User specified scale.
    pub scale: f32,
}

impl BuilderState {
    /// Creates a new layout state.
    pub fn new() -> Self {
        let mut lines = vec![BuilderLine::default()];
        lines[0].styles.push(FragmentStyle::default());
        Self {
            lines,
            ..BuilderState::default()
        }
    }
    #[inline]
    pub fn new_line(&mut self) {
        self.lines.push(BuilderLine::default());
        let last = self.lines.len() - 1;
        self.lines[last]
            .styles
            .push(FragmentStyle::scaled_default(self.scale));
    }
    #[inline]
    pub fn current_line(&self) -> usize {
        let size = self.lines.len();
        if size == 0 {
            0
        } else {
            size - 1
        }
    }
    #[inline]
    pub fn clear(&mut self) {
        self.lines.clear();
        self.features.clear();
        self.vars.clear();
    }

    #[inline]
    pub fn begin(&mut self) {
        self.lines.push(BuilderLine::default());
    }
}

/// Index into a font setting cache.
pub type FontSettingKey = u32;

/// Sentinel for an empty set of font settings.
pub const EMPTY_FONT_SETTINGS: FontSettingKey = !0;

/// Cache of tag/value pairs for font settings.
#[derive(Default)]
pub struct FontSettingCache<T: Copy + PartialOrd + PartialEq> {
    settings: Vec<Setting<T>>,
    lists: Vec<FontSettingList>,
    tmp: Vec<Setting<T>>,
}

impl<T: Copy + PartialOrd + PartialEq> FontSettingCache<T> {
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

/// Range within a font setting cache.
#[derive(Copy, Clone)]
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
