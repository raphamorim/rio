// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use crate::components::rich_text::RichTextBrush;
use crate::font::FontLibrary;
use crate::font_introspector::shape::cluster::OwnedGlyphCluster;
use crate::font_introspector::shape::ShapeContext;
use crate::font_introspector::text::Script;
use crate::font_introspector::Metrics;
use crate::font_introspector::{shape::cluster::GlyphCluster, FontRef};
use crate::layout::render_data::RenderData;
use crate::layout::RichTextLayout;
use crate::Graphics;
use lru::LruCache;
use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::font_introspector::Attributes;
use crate::font_introspector::Setting;
use crate::{sugarloaf::primitives::SugarCursor, DrawableChar, Graphic};

pub struct RichTextCounter(AtomicUsize);

impl RichTextCounter {
    pub const fn new() -> Self {
        Self(AtomicUsize::new(1))
    }

    pub fn next(&self) -> usize {
        self.0.fetch_add(1, Ordering::Relaxed)
    }
}

#[derive(Debug, Clone)]
pub struct FragmentData {
    pub content: String,
    pub style: FragmentStyle,
}

#[derive(Default, Clone, Debug)]
pub struct BuilderLine {
    pub fragments: Vec<FragmentData>,
    pub render_data: RenderData,
}

#[derive(Default, Clone, PartialEq)]
#[repr(C)]
pub enum BuilderStateUpdate {
    #[default]
    Full,
    Partial(HashSet<usize>),
    Noop,
}

#[derive(Default)]
pub struct BuilderState {
    pub lines: Vec<BuilderLine>,
    pub vars: FontSettingCache<f32>,
    pub last_update: BuilderStateUpdate,
    metrics_cache: MetricsCache,
    scaled_font_size: f32,
    pub layout: RichTextLayout,
}

impl BuilderState {
    #[inline]
    pub fn new_line_at(&mut self, pos: usize) {
        self.lines.insert(pos, BuilderLine::default());
    }
    #[inline]
    pub fn remove_line_at(&mut self, pos: usize) {
        self.lines.remove(pos);
    }
    #[inline]
    pub fn from_layout(layout: &RichTextLayout) -> Self {
        Self {
            layout: *layout,
            scaled_font_size: layout.font_size * layout.dimensions.scale,
            ..BuilderState::default()
        }
    }
    #[inline]
    pub fn new_line(&mut self) {
        self.lines.push(BuilderLine::default());
    }
    #[inline]
    pub fn current_line(&self) -> usize {
        self.lines.len().wrapping_sub(1)
    }
    #[inline]
    pub fn mark_clean(&mut self) {
        self.last_update = BuilderStateUpdate::Noop;
    }
    #[inline]
    pub fn mark_dirty(&mut self) {
        self.last_update = BuilderStateUpdate::Full;
    }
    #[inline]
    pub fn mark_line_dirty(&mut self, line: usize) {
        match &mut self.last_update {
            BuilderStateUpdate::Full => {
                // No operation
            }
            BuilderStateUpdate::Noop => {
                self.last_update = BuilderStateUpdate::Partial(HashSet::from([line]));
            }
            BuilderStateUpdate::Partial(set) => {
                set.insert(line);
            }
        };
    }
    #[inline]
    pub fn clear(&mut self) {
        self.lines.clear();
        self.vars.clear();
        self.last_update = BuilderStateUpdate::Full;
    }
    #[inline]
    pub fn rescale(&mut self, scale_factor: f32) {
        self.metrics_cache.inner.clear();
        self.scaled_font_size = self.layout.font_size * scale_factor;
        self.layout.rescale(scale_factor);
    }
    #[inline]
    pub fn begin(&mut self) {
        self.lines.push(BuilderLine::default());
    }
    #[inline]
    pub fn update_font_size(&mut self) {
        let font_size = self.layout.font_size;
        let scale = self.layout.dimensions.scale;
        let prev_font_size = self.scaled_font_size;
        self.scaled_font_size = font_size * scale;

        if prev_font_size != self.scaled_font_size {
            self.metrics_cache.inner.clear();
        }
        self.last_update = BuilderStateUpdate::Full;
    }

    pub fn increase_font_size(&mut self) -> bool {
        if self.layout.font_size < 100.0 {
            self.layout.font_size += 1.0;
            self.update_font_size();
            return true;
        }
        false
    }

    pub fn decrease_font_size(&mut self) -> bool {
        if self.layout.font_size > 6.0 {
            self.layout.font_size -= 1.0;
            self.update_font_size();
            return true;
        }
        false
    }

    pub fn reset_font_size(&mut self) -> bool {
        if self.layout.font_size != self.layout.original_font_size {
            self.layout.font_size = self.layout.original_font_size;
            self.update_font_size();
            return true;
        }
        false
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
    /// Drawable character
    pub drawable_char: Option<DrawableChar>,
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
            drawable_char: None,
        }
    }
}

/// Context for paragraph layout.
pub struct Content {
    fonts: FontLibrary,
    font_features: Vec<crate::font_introspector::Setting<u16>>,
    scx: ShapeContext,
    pub states: FxHashMap<usize, BuilderState>,
    word_cache: WordCache,
    selector: Option<usize>,
    counter: RichTextCounter,
}

impl Content {
    /// Creates a new layout context with the specified font library.
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            fonts: font_library.clone(),
            scx: ShapeContext::new(),
            states: FxHashMap::default(),
            word_cache: WordCache::new(),
            font_features: vec![],
            selector: None,
            counter: RichTextCounter::new(),
        }
    }

    #[inline]
    pub fn sel(&mut self, state_id: usize) -> &mut Content {
        self.selector = Some(state_id);

        self
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        &self.fonts
    }

    #[inline]
    pub fn set_font_library(&mut self, font_library: &FontLibrary) {
        self.fonts = font_library.clone();
        self.word_cache = WordCache::new();
        for line in self.states.values_mut() {
            line.metrics_cache = MetricsCache::default();
        }
    }

    #[inline]
    pub fn get_state(&self, state_id: &usize) -> Option<&BuilderState> {
        self.states.get(state_id)
    }

    #[inline]
    pub fn get_state_mut(&mut self, state_id: &usize) -> Option<&mut BuilderState> {
        self.states.get_mut(state_id)
    }

    #[inline]
    pub fn set_font_features(
        &mut self,
        font_features: Vec<crate::font_introspector::Setting<u16>>,
    ) {
        self.font_features = font_features;
    }

    #[inline]
    pub fn create_state(&mut self, rich_text_layout: &RichTextLayout) -> usize {
        let id = self.counter.next();
        self.states
            .insert(id, BuilderState::from_layout(rich_text_layout));
        id
    }

    #[inline]
    pub fn remove_state(&mut self, rich_text_id: &usize) {
        self.states.remove(rich_text_id);
    }

    #[inline]
    pub fn mark_states_clean(&mut self) {
        for state in self.states.values_mut() {
            state.mark_clean();
        }
    }

    #[inline]
    pub fn update_dimensions(
        &mut self,
        state_id: &usize,
        advance_brush: &mut RichTextBrush,
    ) {
        let mut content = Content::new(&self.fonts);
        if let Some(rte) = self.states.get_mut(state_id) {
            let id = content.create_state(&rte.layout);
            content
                .sel(id)
                .new_line()
                .add_text(" ", FragmentStyle::default())
                .build();
            let render_data = content.get_state(&id).unwrap().lines[0].clone();

            if let Some(dimension) = advance_brush.dimensions(
                &self.fonts,
                &render_data,
                &mut Graphics::default(),
            ) {
                rte.layout.dimensions.height = dimension.height;
                rte.layout.dimensions.width = dimension.width;
            }
        }
    }

    #[inline]
    pub fn clear_state(&mut self, id: &usize) {
        if let Some(state) = self.states.get_mut(id) {
            state.clear();
            state.begin();
        }
    }

    #[inline]
    pub fn new_line_with_id(&mut self, id: &usize) -> &mut Content {
        if let Some(content) = self.states.get_mut(id) {
            content.new_line();
        }

        self
    }

    #[inline]
    pub fn new_line(&mut self) -> &mut Content {
        if let Some(selector) = self.selector {
            return self.new_line_with_id(&selector);
        }

        self
    }

    #[inline]
    pub fn new_line_at(&mut self, pos: usize) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(content) = self.states.get_mut(&selector) {
                content.new_line_at(pos);
            }
        }

        self
    }

    #[inline]
    pub fn remove_line_at(&mut self, pos: usize) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(content) = self.states.get_mut(&selector) {
                content.remove_line_at(pos);
            }
        }

        self
    }

    #[inline]
    pub fn clear_line(&mut self, line_to_clear: usize) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(state) = self.states.get_mut(&selector) {
                if let Some(line) = state.lines.get_mut(line_to_clear) {
                    line.fragments.clear();
                    line.render_data.clear();
                }
            }
        }

        self
    }

    #[inline]
    pub fn clear_with_id(&mut self, id: &usize) -> &mut Content {
        if let Some(state) = self.states.get_mut(id) {
            state.clear();
            state.begin();
        }

        self
    }

    #[inline]
    pub fn clear_all(&mut self) -> &mut Content {
        for state in &mut self.states.values_mut() {
            state.clear();
            state.begin();
        }

        self
    }

    #[inline]
    pub fn clear(&mut self) -> &mut Content {
        if let Some(selector) = self.selector {
            return self.clear_with_id(&selector);
        }

        self
    }

    #[inline]
    pub fn add_text(&mut self, text: &str, style: FragmentStyle) -> &mut Content {
        if let Some(selector) = self.selector {
            return self.add_text_with_id(&selector, text, style);
        }

        self
    }

    #[inline]
    pub fn add_text_on_line(
        &mut self,
        line_idx: usize,
        text: &str,
        style: FragmentStyle,
    ) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(state) = self.states.get_mut(&selector) {
                state.mark_line_dirty(line_idx);
                if let Some(line) = state.lines.get_mut(line_idx) {
                    line.fragments.push(FragmentData {
                        content: text.to_string(),
                        style,
                    });
                }
            }
        }

        self
    }

    /// Adds a text fragment to the paragraph.
    pub fn add_text_with_id(
        &mut self,
        id: &usize,
        text: &str,
        style: FragmentStyle,
    ) -> &mut Content {
        if let Some(state) = self.states.get_mut(id) {
            let current_line = state.current_line();
            if let Some(line) = &mut state.lines.get_mut(current_line) {
                line.fragments.push(FragmentData {
                    content: text.to_string(),
                    style,
                });
            }
        }

        self
    }

    // Helper function to process a single line that avoids borrow issues
    fn process_line(&mut self, state_id: usize, line_number: usize) {
        // Get all needed data while borrowing parts of self separately
        let script = Script::Latin;

        // Safe to get state first as we'll only use it to access properties
        let state = match self.states.get_mut(&state_id) {
            Some(state) => state,
            None => return,
        };

        // Get references to the scaled font size and features outside any other borrows
        let scaled_font_size = state.scaled_font_size;
        let features = &self.font_features;

        // Check if the line exists
        if line_number >= state.lines.len() {
            return;
        }

        // Process fragments in the line
        let line = &mut state.lines[line_number];

        // Process each fragment
        for fragment_idx in 0..line.fragments.len() {
            // Get a reference to the current fragment
            let item = &line.fragments[fragment_idx];
            let font_id = item.style.font_id;
            let font_vars = item.style.font_vars;
            let content = &item.content;
            let style = item.style;

            // Get vars for this fragment
            let vars: Vec<_> = state.vars.get(font_vars).to_vec();

            // Check if the shaped text is already in the cache
            if let Some(cache_entry) = self.word_cache.get(&font_id, content) {
                if let Some(metrics) = state.metrics_cache.inner.get(&font_id) {
                    if line.render_data.push_run_without_shaper(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        cache_entry,
                        metrics,
                    ) {
                        continue;
                    }
                }
            }

            // If not in cache, shape the text
            // Set up cache entry info
            self.word_cache.font_id = font_id;
            self.word_cache.content = content.clone();

            // Process the font data directly without cloning FontRef
            {
                let font_library = &self.fonts.inner.read();
                if let Some((shared_data, offset, key)) = font_library.get_data(&font_id)
                {
                    let font_ref = FontRef {
                        data: shared_data.as_ref(),
                        offset,
                        key,
                    };
                    let mut shaper = self
                        .scx
                        .builder(font_ref) // Use reference directly without cloning
                        .script(script)
                        .size(scaled_font_size)
                        .features(features.iter().copied())
                        .variations(vars.iter().copied())
                        .build();

                    shaper.add_str(content);

                    // Cache metrics if needed
                    state
                        .metrics_cache
                        .inner
                        .entry(font_id)
                        .or_insert_with(|| shaper.metrics());

                    // Push run to render data
                    line.render_data.push_run(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        shaper,
                        &mut self.word_cache,
                    );
                }
            }
        }
    }

    #[inline]
    pub fn build(&mut self) {
        // let start = std::time::Instant::now();
        if let Some(selector) = self.selector {
            let state_id = selector;

            if let Some(state) = self.states.get_mut(&state_id) {
                state.mark_dirty();
                for line_number in 0..state.lines.len() {
                    self.process_line(state_id, line_number);
                }
            }
        }

        // let duration = start.elapsed();
        // println!("Time elapsed in build() is: {:?}", duration);
    }

    #[inline]
    pub fn build_line(&mut self, line_number: usize) {
        if let Some(selector) = self.selector {
            // Process just the specified line
            self.process_line(selector, line_number);
        }
    }
}

#[derive(Default)]
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
        content: &str,
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
                cache.put(
                    std::mem::take(&mut self.content),
                    std::mem::take(&mut self.stash),
                );
            } else {
                // If font id is main
                let size = if self.font_id == 0 { 512 } else { 128 };
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
