// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::font::FontLibrary;
use crate::font_introspector::shape::cluster::GlyphCluster;
use crate::font_introspector::shape::cluster::OwnedGlyphCluster;
use crate::font_introspector::shape::ShapeContext;
use crate::font_introspector::text::Script;
use crate::font_introspector::Metrics;
use crate::layout::render_data::RenderData;
use lru::LruCache;
use rustc_hash::FxHashMap;
use std::num::NonZeroUsize;

use crate::font_introspector::Attributes;
use crate::font_introspector::Setting;
use crate::{sugarloaf::primitives::SugarCursor, Graphic};

/// Data that describes a fragment.
#[derive(Debug, Clone)]
pub struct FragmentData {
    pub content: String,
    /// Style
    pub style: FragmentStyle,
}

#[derive(Default)]
pub struct BuilderLine {
    /// Collection of fragments.
    pub fragments: Vec<FragmentData>,
}

/// Builder state.
#[derive(Default)]
pub struct BuilderState {
    /// Lines State
    pub lines: Vec<BuilderLine>,
    /// Font variation setting cache.
    pub vars: FontSettingCache<f32>,
    /// User specified scale.
    pub scale: f32,
    // Font size in ppem.
    pub font_size: f32,
}

impl BuilderState {
    /// Creates a new layout state.
    pub fn new() -> Self {
        let lines = vec![BuilderLine::default()];
        Self {
            lines,
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
        self.vars.clear();
    }

    #[inline]
    pub fn begin(&mut self) {
        self.lines.push(BuilderLine::default());
    }
}

/// Index into a font setting cache.
pub type FontSettingKey = u32;

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

/// Sentinel for an empty set of font settings.
pub const EMPTY_FONT_SETTINGS: FontSettingKey = !0;

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
    pub offset: f32,
    pub size: f32,
    pub is_doubled: bool,
    pub shape: UnderlineShape,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FragmentStyleDecoration {
    // offset, size
    Underline(UnderlineInfo),
    Strikethrough,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct FragmentStyle {
    pub font_id: usize,
    //  Unicode width
    pub width: f32,
    /// Font attributes.
    pub font_attrs: Attributes,
    /// Font color.
    pub color: [f32; 4],
    /// Background color.
    pub background_color: Option<[f32; 4]>,
    /// Font variations.
    pub font_vars: FontSettingKey,
    /// Additional spacing between letters (clusters) of text.
    // pub letter_spacing: f32,
    /// Additional spacing between words of text.
    // pub word_spacing: f32,
    /// Multiplicative line spacing factor.
    // pub line_spacing: f32,
    /// Enable underline decoration.
    pub decoration: Option<FragmentStyleDecoration>,
    /// Decoration color.
    pub decoration_color: Option<[f32; 4]>,
    /// Cursor style.
    pub cursor: Option<SugarCursor>,
    /// Media
    pub media: Option<Graphic>,
}

impl Default for FragmentStyle {
    fn default() -> Self {
        Self {
            font_id: 0,
            width: 1.0,
            font_attrs: Attributes::default(),
            font_vars: EMPTY_FONT_SETTINGS,
            // letter_spacing: 0.,
            // word_spacing: 0.,
            // line_spacing: 1.,
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            cursor: None,
            decoration: None,
            decoration_color: None,
            media: None,
        }
    }
}

/// Context for paragraph layout.
pub struct Content {
    fonts: FontLibrary,
    font_features: Vec<crate::font_introspector::Setting<u16>>,
    scx: ShapeContext,
    state: BuilderState,
    word_cache: WordCache,
    metrics_cache: MetricsCache,
}

impl Content {
    /// Creates a new layout context with the specified font library.
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            fonts: font_library.clone(),
            scx: ShapeContext::new(),
            state: BuilderState::new(),
            word_cache: WordCache::new(),
            font_features: vec![],
            metrics_cache: MetricsCache::default(),
        }
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        &self.fonts
    }

    #[inline]
    pub fn set_font_features(
        &mut self,
        font_features: Vec<crate::font_introspector::Setting<u16>>,
    ) {
        self.font_features = font_features;
    }

    #[inline]
    pub fn build(&mut self, scale: f32, font_size: f32) {
        self.state.clear();
        self.state.begin();
        let prev_font_size = self.state.font_size;
        self.state.scale = scale;
        self.state.font_size = font_size * scale;

        if prev_font_size != self.state.font_size {
            self.metrics_cache.inner.clear();
        }
        // ContentBuilder {
        //     font_features: &self.font_features,
        //     fonts: &self.fonts,
        //     scx: &mut self.scx,
        //     s: &mut self.state,
        //     word_cache: &mut self.word_cache,
        //     metrics_cache: &mut self.metrics_cache,
        // }
    }

    #[inline]
    pub fn new_line(&mut self) {
        self.state.new_line();
    }

    /// Adds a text fragment to the paragraph.
    pub fn add_text(&mut self, text: &str, style: FragmentStyle) -> Option<()> {
        let current_line = self.state.current_line();
        let line = &mut self.state.lines[current_line];

        line.fragments.push(FragmentData {
            content: text.to_string(),
            style,
        });

        Some(())
    }

    #[inline]
    pub fn resolve(&mut self, render_data: &mut RenderData) {
        let script = Script::Latin;
        for line_number in 0..self.state.lines.len() {
            let line = &self.state.lines[line_number];
            for item in &line.fragments {
                let vars = self.state.vars.get(item.style.font_vars);
                let shaper_key = &item.content;

                // println!("{:?} -> {:?}", item.style.font_id, shaper_key);

                if let Some(shaper) = self.word_cache.get(&item.style.font_id, shaper_key)
                {
                    if let Some(metrics) =
                        self.metrics_cache.inner.get(&item.style.font_id)
                    {
                        if render_data.push_run_without_shaper(
                            item.style,
                            self.state.font_size,
                            line_number as u32,
                            shaper,
                            metrics,
                        ) {
                            continue;
                        }
                    }
                }

                self.word_cache.font_id = item.style.font_id;
                self.word_cache.content = item.content.clone();
                let font_library = { &mut self.fonts.inner.lock() };
                if let Some(data) = font_library.get_data(&item.style.font_id) {
                    let mut shaper = self
                        .scx
                        .builder(data)
                        .script(script)
                        .size(self.state.font_size)
                        .features(self.font_features.iter().copied())
                        .variations(vars.iter().copied())
                        .build();

                    shaper.add_str(&self.word_cache.content);

                    self.metrics_cache
                        .inner
                        .entry(item.style.font_id)
                        .or_insert_with(|| shaper.metrics());

                    render_data.push_run(
                        item.style,
                        self.state.font_size,
                        line_number as u32,
                        shaper,
                        &mut self.word_cache,
                    );
                }
            }
        }
    }
}

pub struct WordCache {
    pub inner: FxHashMap<usize, LruCache<String, Vec<OwnedGlyphCluster>>>,
    stash: Vec<OwnedGlyphCluster>,
    font_id: usize,
    content: String,
}

impl WordCache {
    pub fn new() -> Self {
        WordCache {
            inner: FxHashMap::default(),
            stash: vec![],
            font_id: 0,
            content: String::new(),
        }
    }

    #[inline]
    pub fn get(
        &mut self,
        font_id: &usize,
        content: &String,
    ) -> Option<&Vec<OwnedGlyphCluster>> {
        if let Some(cache) = self.inner.get_mut(font_id) {
            return cache.get(content);
        }
        None
    }

    #[inline]
    pub fn add_glyph_cluster(&mut self, glyph_cluster: &GlyphCluster) {
        self.stash.push(glyph_cluster.into());
    }

    #[inline]
    pub fn finish(&mut self) {
        if !self.content.is_empty() && !self.stash.is_empty() {
            if let Some(cache) = self.inner.get_mut(&self.font_id) {
                println!("{:?} {:?}", self.content, cache.len());
                cache.put(
                    std::mem::take(&mut self.content),
                    std::mem::take(&mut self.stash),
                );
            } else {
                // If font id is main
                let size = if self.font_id == 0 {
                    384
                } else {
                    128
                };
                let mut cache = LruCache::new(NonZeroUsize::new(size).unwrap());
                cache.put(
                    std::mem::take(&mut self.content),
                    std::mem::take(&mut self.stash),
                );
                self.inner.insert(self.font_id, cache);
            }

            self.font_id = 0;
            return;
        }
        self.stash.clear();
        self.font_id = 0;
        self.content.clear();
    }
}

#[derive(Default)]
struct MetricsCache {
    pub inner: FxHashMap<usize, Metrics>,
}
