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
#[derive(Copy, Clone)]
pub struct FragmentData {
    /// Identifier of the span that contains the fragment.
    pub span: FragmentStyle,
    /// True if this fragment breaks shaping with the previous fragment.
    pub break_shaping: bool,
    /// True if this fragment is text.
    pub is_text: bool,
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
    pub spans: Vec<FragmentStyle>,
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
        Self {
            lines: vec![BuilderLine::default()],
            ..BuilderState::default()
        }
    }
    #[inline]
    pub fn new_line(&mut self) {
        self.lines.push(BuilderLine::default());
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
    pub fn add<I>(&mut self, settings: I) -> FontSettingKey
    where
        I: Iterator,
        I::Item: Into<Setting<T>>,
    {
        self.tmp.clear();
        self.tmp.extend(settings.map(|v| v.into()));
        let len = self.tmp.len();
        if len == 0 {
            return EMPTY_FONT_SETTINGS;
        }
        self.tmp.sort_unstable_by(|a, b| a.tag.cmp(&b.tag));
        'outer: for (i, list) in self.lists.iter().enumerate() {
            let other = list.get(&self.settings);
            if other.len() != len {
                continue;
            }
            for (a, b) in self.tmp.iter().zip(other) {
                if a.tag != b.tag || a.value != b.value {
                    continue 'outer;
                }
            }
            return i as u32;
        }
        let key = self.lists.len() as u32;
        let start = self.settings.len() as u32;
        self.settings.extend_from_slice(&self.tmp);
        let end = self.settings.len() as u32;
        self.lists.push(FontSettingList { start, end });
        key
    }

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
