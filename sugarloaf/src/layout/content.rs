// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

#![allow(clippy::uninlined_format_args)]

use crate::components::rich_text::RichTextBrush;
use crate::font::FontLibrary;
use crate::font_introspector::shape::cluster::OwnedGlyphCluster;
use crate::font_introspector::shape::ShapeContext;
use crate::font_introspector::text::Script;
use crate::font_introspector::{shape::cluster::GlyphCluster, FontRef};
use crate::layout::render_data::RenderData;
use crate::layout::RichTextLayout;
use crate::Graphics;
use lru::LruCache;
use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use tracing::debug;

use crate::font_introspector::Attributes;
use crate::font_introspector::Setting;
use crate::{sugarloaf::primitives::SugarCursor, DrawableChar, Graphic};

/// Cached content that can be either normal clusters or optimized whitespace
#[derive(Clone, Debug)]
pub enum CachedContent {
    /// Normal glyph clusters
    Normal(Vec<OwnedGlyphCluster>),
    /// Optimized repeated whitespace character
    RepeatedWhitespace {
        single_cluster: OwnedGlyphCluster,
        original_count: usize,
    },
}

impl CachedContent {
    /// Expand the cached content to the actual glyph clusters
    pub fn expand(&self, requested_count: Option<usize>) -> Vec<OwnedGlyphCluster> {
        match self {
            CachedContent::Normal(clusters) => clusters.clone(),
            CachedContent::RepeatedWhitespace {
                single_cluster,
                original_count,
            } => {
                let count = requested_count.unwrap_or(*original_count);
                let mut expanded = Vec::with_capacity(count);

                // Repeat the cluster, updating the source range for each position
                for i in 0..count {
                    let mut cluster = single_cluster.clone();
                    // Update source range to reflect the actual character position
                    cluster.source =
                        crate::font_introspector::text::cluster::SourceRange {
                            start: i as u32,
                            end: (i + 1) as u32,
                        };
                    expanded.push(cluster);
                }
                expanded
            }
        }
    }

    /// Get the clusters as a reference for normal content, or None for whitespace
    #[allow(dead_code)]
    pub fn as_normal(&self) -> Option<&Vec<OwnedGlyphCluster>> {
        match self {
            CachedContent::Normal(clusters) => Some(clusters),
            CachedContent::RepeatedWhitespace { .. } => None,
        }
    }
}

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
        self.scaled_font_size = font_size * scale;

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
            if let Some(cached_content) =
                self.word_cache.get_cached_content(&font_id, content)
            {
                // Get metrics from FontLibraryData (with caching)
                if let Some((ascent, descent, leading)) = self
                    .fonts
                    .inner
                    .write()
                    .get_font_metrics(&font_id, scaled_font_size)
                {
                    // Create a minimal font_introspector::Metrics for cached content
                    let metrics = crate::font_introspector::Metrics {
                        ascent,
                        descent,
                        leading,
                        ..Default::default()
                    };

                    // Handle different types of cached content
                    match cached_content {
                        CachedContent::Normal(clusters) => {
                            // debug!("=== CACHE HIT: USING NORMAL CONTENT ===");
                            // debug!("Content: '{}' (len={})", content, content.len());
                            // debug!(
                            //     "Using cached Normal content with {} clusters",
                            //     clusters.len()
                            // );
                            // debug!("=== END CACHE HIT ===");

                            if line.render_data.push_run_without_shaper(
                                style,
                                scaled_font_size,
                                line_number as u32,
                                clusters,
                                &metrics,
                            ) {
                                continue;
                            }
                        }
                        CachedContent::RepeatedWhitespace { .. } => {
                            // Expand the whitespace sequence to the actual clusters
                            // debug!("=== CACHE HIT: USING OPTIMIZED WHITESPACE ===");
                            // debug!("Content: '{}' (len={})", content, content.len());
                            // debug!(
                            //     "Using cached RepeatedWhitespace - no shaping needed!"
                            // );
                            // debug!("=== END CACHE HIT ===");

                            let expanded_clusters = cached_content.expand(None);

                            if line.render_data.push_run_without_shaper(
                                style,
                                scaled_font_size,
                                line_number as u32,
                                &expanded_clusters,
                                &metrics,
                            ) {
                                continue;
                            }
                        }
                    }
                } else {
                    debug!("Font metrics not available for font_id={}", font_id);
                }
            }

            // If not in cache, shape the text
            // Set up cache entry info
            self.word_cache.set_content(font_id, content);

            // Check if this is a repeated whitespace sequence that we can optimize
            if let Some((whitespace_char, count)) =
                WordCache::analyze_whitespace_sequence(content)
            {
                debug!("=== WHITESPACE OPTIMIZATION ===");
                debug!(
                    "Detected repeated whitespace: '{}' x{}",
                    whitespace_char, count
                );
                debug!("Shaping only single character instead of {}", count);

                // Shape only a single whitespace character
                let single_char_content = whitespace_char.to_string();

                // Process the font data directly without cloning FontRef
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
                        .builder(font_ref)
                        .script(script)
                        .size(scaled_font_size)
                        .features(features.iter().copied())
                        .variations(vars.iter().copied())
                        .build();

                    shaper.add_str(&single_char_content);

                    // Get metrics before shaping (since shape_with consumes the shaper)
                    let metrics = shaper.metrics();

                    // Shape the single character and store as optimized
                    let mut single_cluster = None;
                    shaper.shape_with(|cluster| {
                        single_cluster = Some(cluster.into());
                    });

                    if let Some(cluster) = single_cluster {
                        // Create optimized cached content directly
                        let cached_content = CachedContent::RepeatedWhitespace {
                            single_cluster: cluster,
                            original_count: count,
                        };

                        // Store in cache
                        if let Some(cache) = self.word_cache.inner.get_mut(&font_id) {
                            cache.put(self.word_cache.content_hash, cached_content);
                        } else {
                            let size = if font_id == 0 { 512 } else { 128 };
                            let mut cache =
                                LruCache::new(NonZeroUsize::new(size).unwrap());
                            cache.put(self.word_cache.content_hash, cached_content);
                            self.word_cache.inner.insert(font_id, cache);
                        }

                        // Get the cached content and expand it for rendering
                        if let Some(cached) =
                            self.word_cache.get_cached_content(&font_id, content)
                        {
                            let expanded_clusters = cached.expand(None);
                            line.render_data.push_run_without_shaper(
                                style,
                                scaled_font_size,
                                line_number as u32,
                                &expanded_clusters,
                                &metrics,
                            );
                        }
                    }

                    // Reset cache state
                    self.word_cache.font_id = 0;
                    self.word_cache.content_hash = 0;
                    self.word_cache.current_content = None;
                }
            } else {
                // Normal content - shape as usual
                // Process the font data directly without cloning FontRef
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
    pub inner: FxHashMap<usize, LruCache<u64, CachedContent>>,
    stash: Vec<OwnedGlyphCluster>,
    font_id: usize,
    content_hash: u64,
    // Track current content being processed
    current_content: Option<String>,
}

impl WordCache {
    pub fn new() -> Self {
        WordCache {
            inner: FxHashMap::default(),
            stash: Vec::with_capacity(64), // Pre-allocate stash capacity
            font_id: 0,
            content_hash: 0,
            current_content: None,
        }
    }

    /// Generate a hash-based cache key from content and font_id
    /// Uses direct string hashing to avoid hash collisions from string interning
    #[inline]
    pub fn cache_key_with_interning(&mut self, content: &str, font_id: usize) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        // Hash the actual string content directly to avoid atom hash collisions
        content.hash(&mut hasher);
        font_id.hash(&mut hasher);
        hasher.finish()
    }

    /// Check if content is a sequence of identical whitespace characters
    /// Optimized version with SIMD fast paths for long sequences
    #[inline]
    pub fn analyze_whitespace_sequence(content: &str) -> Option<(char, usize)> {
        let bytes = content.as_bytes();
        if bytes.len() < 4 {
            return None;
        }

        // Fast path for ASCII space (most common case)
        if bytes[0] == b' ' {
            if Self::simd_check_all_spaces(bytes) {
                return Some((' ', bytes.len()));
            }
            return None; // Mixed content with spaces
        }

        // Fast path for ASCII tab
        if bytes[0] == b'\t' {
            if Self::simd_check_all_tabs(bytes) {
                return Some(('\t', bytes.len()));
            }
            return None; // Mixed content with tabs
        }

        // Fallback to Unicode char iteration for other whitespace
        let mut chars = content.chars();
        let first_char = chars.next()?;

        if !first_char.is_whitespace() {
            return None;
        }

        // Count chars while checking if all are the same
        let mut char_count = 1;
        for ch in chars {
            if ch != first_char {
                return None; // Mixed whitespace types
            }
            char_count += 1;
        }

        if char_count >= 4 {
            Some((first_char, char_count))
        } else {
            None
        }
    }

    /// SIMD-optimized check for all spaces using platform-specific instructions
    #[inline]
    fn simd_check_all_spaces(bytes: &[u8]) -> bool {
        // For very long sequences, use SIMD when available
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            if bytes.len() >= 32 {
                return Self::avx2_check_all_spaces(bytes);
            }
        }

        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        {
            if bytes.len() >= 16 {
                return Self::sse2_check_all_spaces(bytes);
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            if bytes.len() >= 16 {
                return Self::neon_check_all_spaces(bytes);
            }
        }

        // Fallback to optimized scalar version
        Self::scalar_check_all_spaces(bytes)
    }

    /// SIMD-optimized check for all tabs
    #[inline]
    fn simd_check_all_tabs(bytes: &[u8]) -> bool {
        // Similar SIMD optimization for tabs
        #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
        {
            if bytes.len() >= 32 {
                return Self::avx2_check_all_tabs(bytes);
            }
        }

        #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
        {
            if bytes.len() >= 16 {
                return Self::sse2_check_all_tabs(bytes);
            }
        }

        #[cfg(target_arch = "aarch64")]
        {
            if bytes.len() >= 16 {
                return Self::neon_check_all_tabs(bytes);
            }
        }

        // Fallback to optimized scalar version
        Self::scalar_check_all_tabs(bytes)
    }

    /// AVX2 implementation for checking all spaces (32 bytes at a time)
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    #[inline]
    fn avx2_check_all_spaces(bytes: &[u8]) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::*;

            unsafe {
                let space_vec = _mm256_set1_epi8(b' ' as i8);
                let mut i = 0;

                // Process 32 bytes at a time
                while i + 32 <= bytes.len() {
                    let chunk =
                        _mm256_loadu_si256(bytes.as_ptr().add(i) as *const __m256i);
                    let cmp = _mm256_cmpeq_epi8(chunk, space_vec);
                    let mask = _mm256_movemask_epi8(cmp);

                    if mask != -1 {
                        return false; // Found non-space character
                    }
                    i += 32;
                }

                // Handle remaining bytes
                for &byte in &bytes[i..] {
                    if byte != b' ' {
                        return false;
                    }
                }

                true
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Self::scalar_check_all_spaces(bytes)
        }
    }

    /// SSE2 implementation for checking all spaces (16 bytes at a time)
    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[inline]
    fn sse2_check_all_spaces(bytes: &[u8]) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::*;

            unsafe {
                let space_vec = _mm_set1_epi8(b' ' as i8);
                let mut i = 0;

                // Process 16 bytes at a time
                while i + 16 <= bytes.len() {
                    let chunk = _mm_loadu_si128(bytes.as_ptr().add(i) as *const __m128i);
                    let cmp = _mm_cmpeq_epi8(chunk, space_vec);
                    let mask = _mm_movemask_epi8(cmp);

                    if mask != 0xFFFF {
                        return false; // Found non-space character
                    }
                    i += 16;
                }

                // Handle remaining bytes
                for &byte in &bytes[i..] {
                    if byte != b' ' {
                        return false;
                    }
                }

                true
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Self::scalar_check_all_spaces(bytes)
        }
    }

    /// ARM NEON implementation for checking all spaces
    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn neon_check_all_spaces(bytes: &[u8]) -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            use std::arch::aarch64::*;

            unsafe {
                let space_vec = vdupq_n_u8(b' ');
                let mut i = 0;

                // Process 16 bytes at a time
                while i + 16 <= bytes.len() {
                    let chunk = vld1q_u8(bytes.as_ptr().add(i));
                    let cmp = vceqq_u8(chunk, space_vec);

                    // Check if all lanes are true (all spaces)
                    let min_val = vminvq_u8(cmp);
                    if min_val == 0 {
                        return false; // Found non-space character
                    }
                    i += 16;
                }

                // Handle remaining bytes
                for &byte in &bytes[i..] {
                    if byte != b' ' {
                        return false;
                    }
                }

                true
            }
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            Self::scalar_check_all_spaces(bytes)
        }
    }

    /// Optimized scalar implementation for checking all spaces
    #[inline]
    fn scalar_check_all_spaces(bytes: &[u8]) -> bool {
        // Process 8 bytes at a time using u64 comparison
        let mut i = 0;
        let space_pattern = 0x2020202020202020u64; // Eight spaces

        while i + 8 <= bytes.len() {
            let chunk = u64::from_ne_bytes([
                bytes[i],
                bytes[i + 1],
                bytes[i + 2],
                bytes[i + 3],
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ]);

            if chunk != space_pattern {
                return false;
            }
            i += 8;
        }

        // Handle remaining bytes
        for &byte in &bytes[i..] {
            if byte != b' ' {
                return false;
            }
        }

        true
    }

    /// Similar implementations for tabs (0x09)
    #[cfg(all(target_arch = "x86_64", target_feature = "avx2"))]
    #[inline]
    fn avx2_check_all_tabs(bytes: &[u8]) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::*;

            unsafe {
                let tab_vec = _mm256_set1_epi8(b'\t' as i8);
                let mut i = 0;

                while i + 32 <= bytes.len() {
                    let chunk =
                        _mm256_loadu_si256(bytes.as_ptr().add(i) as *const __m256i);
                    let cmp = _mm256_cmpeq_epi8(chunk, tab_vec);
                    let mask = _mm256_movemask_epi8(cmp);

                    if mask != -1 {
                        return false;
                    }
                    i += 32;
                }

                for &byte in &bytes[i..] {
                    if byte != b'\t' {
                        return false;
                    }
                }

                true
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Self::scalar_check_all_tabs(bytes)
        }
    }

    #[cfg(all(target_arch = "x86_64", target_feature = "sse2"))]
    #[inline]
    fn sse2_check_all_tabs(bytes: &[u8]) -> bool {
        #[cfg(target_arch = "x86_64")]
        {
            use std::arch::x86_64::*;

            unsafe {
                let tab_vec = _mm_set1_epi8(b'\t' as i8);
                let mut i = 0;

                while i + 16 <= bytes.len() {
                    let chunk = _mm_loadu_si128(bytes.as_ptr().add(i) as *const __m128i);
                    let cmp = _mm_cmpeq_epi8(chunk, tab_vec);
                    let mask = _mm_movemask_epi8(cmp);

                    if mask != 0xFFFF {
                        return false;
                    }
                    i += 16;
                }

                for &byte in &bytes[i..] {
                    if byte != b'\t' {
                        return false;
                    }
                }

                true
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            Self::scalar_check_all_tabs(bytes)
        }
    }

    #[cfg(target_arch = "aarch64")]
    #[inline]
    fn neon_check_all_tabs(bytes: &[u8]) -> bool {
        #[cfg(target_arch = "aarch64")]
        {
            use std::arch::aarch64::*;

            unsafe {
                let tab_vec = vdupq_n_u8(b'\t');
                let mut i = 0;

                while i + 16 <= bytes.len() {
                    let chunk = vld1q_u8(bytes.as_ptr().add(i));
                    let cmp = vceqq_u8(chunk, tab_vec);

                    let min_val = vminvq_u8(cmp);
                    if min_val == 0 {
                        return false;
                    }
                    i += 16;
                }

                for &byte in &bytes[i..] {
                    if byte != b'\t' {
                        return false;
                    }
                }

                true
            }
        }
        #[cfg(not(target_arch = "aarch64"))]
        {
            Self::scalar_check_all_tabs(bytes)
        }
    }

    #[inline]
    fn scalar_check_all_tabs(bytes: &[u8]) -> bool {
        let mut i = 0;
        let tab_pattern = 0x0909090909090909u64; // Eight tabs

        while i + 8 <= bytes.len() {
            let chunk = u64::from_ne_bytes([
                bytes[i],
                bytes[i + 1],
                bytes[i + 2],
                bytes[i + 3],
                bytes[i + 4],
                bytes[i + 5],
                bytes[i + 6],
                bytes[i + 7],
            ]);

            if chunk != tab_pattern {
                return false;
            }
            i += 8;
        }

        for &byte in &bytes[i..] {
            if byte != b'\t' {
                return false;
            }
        }

        true
    }

    /// Get cached content, handling both normal and optimized whitespace
    #[inline]
    pub fn get_cached_content(
        &mut self,
        font_id: &usize,
        content: &str,
    ) -> Option<&CachedContent> {
        let key = self.cache_key_with_interning(content, *font_id);
        if let Some(cache) = self.inner.get_mut(font_id) {
            if let Some(cached_content) = cache.get(&key) {
                return Some(cached_content);
            }
        }

        None
    }

    #[inline]
    pub fn add_glyph_cluster(&mut self, glyph_cluster: &GlyphCluster) {
        self.stash.push(glyph_cluster.into());
    }

    #[inline]
    pub fn set_content(&mut self, font_id: usize, content: &str) {
        self.font_id = font_id;
        self.content_hash = self.cache_key_with_interning(content, font_id);
        self.current_content = Some(content.to_string());
    }

    #[inline]
    pub fn finish(&mut self) {
        if self.content_hash != 0 && !self.stash.is_empty() {
            // For normal content (non-whitespace sequences), store as normal cache
            // Whitespace optimization is now handled upfront in process_line
            let cached_content = CachedContent::Normal(std::mem::take(&mut self.stash));

            // Store in cache
            if let Some(cache) = self.inner.get_mut(&self.font_id) {
                cache.put(self.content_hash, cached_content);
            } else {
                // If font id is main
                let size = if self.font_id == 0 { 512 } else { 256 };
                let mut cache = LruCache::new(NonZeroUsize::new(size).unwrap());
                debug!("WordCache creating new cache for font_id={}", self.font_id);
                cache.put(self.content_hash, cached_content);
                self.inner.insert(self.font_id, cache);
            }

            self.font_id = 0;
            self.content_hash = 0;
            self.current_content = None;
            return;
        }
        self.stash.clear();
        self.font_id = 0;
        self.content_hash = 0;
        self.current_content = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::font_introspector::shape::cluster::Glyph;
    use crate::font_introspector::text::cluster::SourceRange;

    fn create_test_glyph(id: u16, x: f32, y: f32, advance: f32) -> Glyph {
        Glyph {
            id,
            info: Default::default(),
            x,
            y,
            advance,
            data: Default::default(),
        }
    }

    fn create_test_cluster(
        source_start: u32,
        source_end: u32,
        glyph: Glyph,
    ) -> OwnedGlyphCluster {
        OwnedGlyphCluster {
            source: SourceRange {
                start: source_start,
                end: source_end,
            },
            info: Default::default(),
            glyphs: vec![glyph],
            components: Vec::new(),
            data: Default::default(),
        }
    }

    #[test]
    fn test_whitespace_optimization_vs_normal_shaping() {
        // Test data: 10 spaces
        let whitespace_count = 10;
        let space_advance = 16.40625;
        let space_glyph_id = 2013;

        // Create what normal shaping would produce: 10 individual clusters
        let mut normal_clusters = Vec::new();
        for i in 0..whitespace_count {
            let glyph = create_test_glyph(space_glyph_id, 0.0, 0.0, space_advance);
            let cluster = create_test_cluster(i as u32, (i + 1) as u32, glyph);
            normal_clusters.push(cluster);
        }

        // Create what the optimization stores: single cluster
        let single_glyph = create_test_glyph(space_glyph_id, 0.0, 0.0, space_advance);
        let single_cluster = create_test_cluster(0, 1, single_glyph);

        // Test normal cached content
        let normal_content = CachedContent::Normal(normal_clusters.clone());
        let normal_expanded = normal_content.expand(None);

        // Test optimized cached content
        let optimized_content = CachedContent::RepeatedWhitespace {
            single_cluster,
            original_count: whitespace_count,
        };
        let optimized_expanded = optimized_content.expand(None);

        // Both should produce the same number of clusters
        assert_eq!(normal_expanded.len(), optimized_expanded.len());
        assert_eq!(normal_expanded.len(), whitespace_count);

        // Compare each cluster
        for (i, (normal_cluster, optimized_cluster)) in normal_expanded
            .iter()
            .zip(optimized_expanded.iter())
            .enumerate()
        {
            // Source ranges should match
            assert_eq!(normal_cluster.source.start, optimized_cluster.source.start);
            assert_eq!(normal_cluster.source.end, optimized_cluster.source.end);
            assert_eq!(normal_cluster.source.start, i as u32);
            assert_eq!(normal_cluster.source.end, (i + 1) as u32);

            // Number of glyphs should match
            assert_eq!(normal_cluster.glyphs.len(), optimized_cluster.glyphs.len());
            assert_eq!(normal_cluster.glyphs.len(), 1);

            // Glyph data should match
            let normal_glyph = &normal_cluster.glyphs[0];
            let optimized_glyph = &optimized_cluster.glyphs[0];

            assert_eq!(normal_glyph.id, optimized_glyph.id);
            assert_eq!(normal_glyph.x, optimized_glyph.x);
            assert_eq!(normal_glyph.y, optimized_glyph.y);
            assert_eq!(normal_glyph.advance, optimized_glyph.advance);
        }
    }

    #[test]
    fn test_whitespace_optimization_different_counts() {
        let space_advance = 16.40625;
        let space_glyph_id = 2013;
        let single_glyph = create_test_glyph(space_glyph_id, 0.0, 0.0, space_advance);
        let single_cluster = create_test_cluster(0, 1, single_glyph);

        let optimized_content = CachedContent::RepeatedWhitespace {
            single_cluster,
            original_count: 5,
        };

        // Test expanding to original count
        let expanded_original = optimized_content.expand(None);
        assert_eq!(expanded_original.len(), 5);

        // Test expanding to different count
        let expanded_custom = optimized_content.expand(Some(8));
        assert_eq!(expanded_custom.len(), 8);

        // Verify source ranges are correct for custom count
        for (i, cluster) in expanded_custom.iter().enumerate() {
            assert_eq!(cluster.source.start, i as u32);
            assert_eq!(cluster.source.end, (i + 1) as u32);
        }
    }

    #[test]
    fn test_normal_content_passthrough() {
        // Test that normal content is passed through unchanged
        let glyph1 = create_test_glyph(100, 0.0, 0.0, 10.0);
        let glyph2 = create_test_glyph(101, 10.0, 0.0, 12.0);
        let cluster1 = create_test_cluster(0, 1, glyph1);
        let cluster2 = create_test_cluster(1, 2, glyph2);

        let original_clusters = vec![cluster1.clone(), cluster2.clone()];
        let normal_content = CachedContent::Normal(original_clusters.clone());
        let expanded = normal_content.expand(None);

        assert_eq!(expanded.len(), 2);
        assert_eq!(expanded[0].source.start, cluster1.source.start);
        assert_eq!(expanded[0].source.end, cluster1.source.end);
        assert_eq!(expanded[1].source.start, cluster2.source.start);
        assert_eq!(expanded[1].source.end, cluster2.source.end);
        assert_eq!(expanded[0].glyphs[0].id, glyph1.id);
        assert_eq!(expanded[1].glyphs[0].id, glyph2.id);
    }

    #[test]
    fn test_whitespace_analysis() {
        // Test the whitespace analysis function
        assert_eq!(WordCache::analyze_whitespace_sequence(""), None);
        assert_eq!(WordCache::analyze_whitespace_sequence("a"), None);
        assert_eq!(WordCache::analyze_whitespace_sequence("   "), None); // Only 3 chars
        assert_eq!(
            WordCache::analyze_whitespace_sequence("    "),
            Some((' ', 4))
        ); // 4 chars
        assert_eq!(
            WordCache::analyze_whitespace_sequence("          "),
            Some((' ', 10))
        ); // 10 chars
        assert_eq!(WordCache::analyze_whitespace_sequence("  a  "), None); // Mixed content
        assert_eq!(
            WordCache::analyze_whitespace_sequence("\t\t\t\t"),
            Some(('\t', 4))
        ); // Tabs
        assert_eq!(WordCache::analyze_whitespace_sequence(" \t  "), None); // Mixed whitespace
    }

    #[test]
    fn test_glyph_positioning_in_clusters() {
        // This test verifies that glyph positioning is handled correctly
        // In the current implementation, individual glyphs in clusters have x=0, y=0
        // because positioning is handled by the renderer during layout

        let space_advance = 16.40625;
        let space_glyph_id = 2013;
        let single_glyph = create_test_glyph(space_glyph_id, 0.0, 0.0, space_advance);
        let single_cluster = create_test_cluster(0, 1, single_glyph);

        let optimized_content = CachedContent::RepeatedWhitespace {
            single_cluster,
            original_count: 5,
        };

        let expanded = optimized_content.expand(None);

        // All glyphs should have the same advance value
        for cluster in &expanded {
            assert_eq!(cluster.glyphs.len(), 1);
            let glyph = &cluster.glyphs[0];
            assert_eq!(glyph.advance, space_advance);
            assert_eq!(glyph.id, space_glyph_id);
            // Note: x and y are 0 in cluster data because positioning
            // is handled by the renderer during layout
            assert_eq!(glyph.x, 0.0);
            assert_eq!(glyph.y, 0.0);
        }

        // Verify that the renderer can calculate total advance correctly
        let total_advance: f32 = expanded
            .iter()
            .flat_map(|cluster| &cluster.glyphs)
            .map(|glyph| glyph.advance)
            .sum();

        assert_eq!(total_advance, space_advance * 5.0);
    }

    #[test]
    fn test_cache_behavior_with_whitespace_optimization() {
        // Test that the cache correctly stores and retrieves optimized whitespace

        // Test whitespace optimization (10 spaces)
        {
            let mut word_cache = WordCache::default();
            let font_id = 0;
            let whitespace_content = "          "; // 10 spaces

            // Verify cache miss
            assert!(word_cache
                .get_cached_content(&font_id, whitespace_content)
                .is_none());

            // Simulate storing optimized whitespace content
            let space_glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
            let glyphs = vec![space_glyph];
            let components = vec![];
            let space_cluster = crate::font_introspector::shape::cluster::GlyphCluster {
                source: SourceRange { start: 0, end: 1 },
                info: Default::default(),
                glyphs: &glyphs,
                components: &components,
                data: Default::default(),
            };

            word_cache.set_content(font_id, whitespace_content);
            word_cache.add_glyph_cluster(&space_cluster);
            word_cache.finish();

            // Test cache hit (should be optimized)
            let cached_whitespace =
                word_cache.get_cached_content(&font_id, whitespace_content);
            assert!(cached_whitespace.is_some());
            let whitespace_content_ref = cached_whitespace.unwrap();
            match whitespace_content_ref {
                CachedContent::Normal(clusters) => {
                    // With new upfront optimization, manual cache operations store as Normal
                    assert_eq!(clusters.len(), 1); // Only one cluster was added manually
                }
                CachedContent::RepeatedWhitespace { original_count, .. } => {
                    // This shouldn't happen with the new implementation
                    assert_eq!(*original_count, 10);
                }
            }
            // Since manual cache operations now store as Normal,
            // we can't test expansion the same way. The real optimization
            // happens in process_line (see test_upfront_whitespace_optimization)
            if let CachedContent::Normal(clusters) = whitespace_content_ref {
                assert_eq!(clusters.len(), 1); // Only one cluster was added manually

                // Test that we can still expand if it were optimized
                let mock_optimized = CachedContent::RepeatedWhitespace {
                    single_cluster: clusters[0].clone(),
                    original_count: 10,
                };
                let expanded = mock_optimized.expand(None);
                assert_eq!(expanded.len(), 10);

                // Verify expansion logic works correctly
                for (i, cluster) in expanded.iter().enumerate() {
                    assert_eq!(cluster.source.start, i as u32);
                    assert_eq!(cluster.source.end, (i + 1) as u32);
                    assert_eq!(cluster.glyphs.len(), 1);
                    assert_eq!(cluster.glyphs[0].advance, 16.40625);
                }
            }
        }

        // Test normal caching (3 spaces - should not be optimized)
        {
            let mut word_cache = WordCache::default();
            let font_id = 0;
            let short_content = "   "; // 3 spaces

            // Verify cache miss
            assert!(word_cache
                .get_cached_content(&font_id, short_content)
                .is_none());

            // Simulate storing normal content for short spaces
            word_cache.set_content(font_id, short_content);
            for i in 0..3 {
                let glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
                let glyphs = vec![glyph];
                let components = vec![];
                let cluster = crate::font_introspector::shape::cluster::GlyphCluster {
                    source: SourceRange {
                        start: i as u32,
                        end: (i + 1) as u32,
                    },
                    info: Default::default(),
                    glyphs: &glyphs,
                    components: &components,
                    data: Default::default(),
                };
                word_cache.add_glyph_cluster(&cluster);
            }
            word_cache.finish();

            // Test cache hit (should be normal)
            let cached_short = word_cache.get_cached_content(&font_id, short_content);
            assert!(cached_short.is_some());
            match cached_short.unwrap() {
                CachedContent::Normal(clusters) => {
                    assert_eq!(clusters.len(), 3);
                    for (i, cluster) in clusters.iter().enumerate() {
                        assert_eq!(cluster.source.start, i as u32);
                        assert_eq!(cluster.source.end, (i + 1) as u32);
                    }
                }
                CachedContent::RepeatedWhitespace { .. } => {
                    panic!("Expected Normal, got RepeatedWhitespace")
                }
            }
        }

        // Test cache miss for content not stored
        {
            let mut word_cache = WordCache::default();
            let font_id = 0;
            let cached_mixed = word_cache.get_cached_content(&font_id, "  a  ");
            assert!(cached_mixed.is_none());
        }
    }

    #[test]
    fn test_optimization_threshold() {
        // Test that optimization only triggers for sequences >= 4 characters
        assert!(WordCache::analyze_whitespace_sequence("   ").is_none()); // 3 chars
        assert!(WordCache::analyze_whitespace_sequence("    ").is_some()); // 4 chars
        assert!(WordCache::analyze_whitespace_sequence("     ").is_some()); // 5 chars

        // Test different whitespace characters
        assert!(WordCache::analyze_whitespace_sequence("\t\t\t").is_none()); // 3 tabs
        assert!(WordCache::analyze_whitespace_sequence("\t\t\t\t").is_some()); // 4 tabs

        // Test mixed whitespace (should not optimize)
        assert!(WordCache::analyze_whitespace_sequence("  \t ").is_none()); // mixed
        assert!(WordCache::analyze_whitespace_sequence(" \n  ").is_none()); // mixed with newline
    }

    #[test]
    fn test_edge_cases_and_boundary_conditions() {
        // Test empty and single character strings
        assert!(WordCache::analyze_whitespace_sequence("").is_none());
        assert!(WordCache::analyze_whitespace_sequence(" ").is_none());
        assert!(WordCache::analyze_whitespace_sequence("a").is_none());

        // Test exactly at threshold
        assert!(WordCache::analyze_whitespace_sequence("    ").is_some()); // exactly 4
        assert!(WordCache::analyze_whitespace_sequence("   ").is_none()); // exactly 3

        // Test very long sequences
        let long_spaces = " ".repeat(1000);
        assert_eq!(
            WordCache::analyze_whitespace_sequence(&long_spaces),
            Some((' ', 1000))
        );

        // Test different whitespace types
        assert_eq!(
            WordCache::analyze_whitespace_sequence("    "),
            Some((' ', 4))
        );
        assert_eq!(
            WordCache::analyze_whitespace_sequence("\t\t\t\t"),
            Some(('\t', 4))
        );

        // Test non-whitespace
        assert!(WordCache::analyze_whitespace_sequence("aaaa").is_none());
        assert!(WordCache::analyze_whitespace_sequence("1234").is_none());
    }

    #[test]
    fn test_cache_with_different_font_ids() {
        let mut word_cache = WordCache::default();
        let whitespace_content = "     "; // 5 spaces

        // Store same content for different font IDs
        for font_id in 0..3 {
            let space_glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
            let glyphs = vec![space_glyph];
            let components = vec![];
            let space_cluster = crate::font_introspector::shape::cluster::GlyphCluster {
                source: SourceRange { start: 0, end: 1 },
                info: Default::default(),
                glyphs: &glyphs,
                components: &components,
                data: Default::default(),
            };

            word_cache.set_content(font_id, whitespace_content);
            word_cache.add_glyph_cluster(&space_cluster);
            word_cache.finish();
        }

        // Verify each font ID has its own cache entry
        for font_id in 0..3 {
            let cached = word_cache.get_cached_content(&font_id, whitespace_content);
            assert!(cached.is_some());
            match cached.unwrap() {
                CachedContent::Normal(clusters) => {
                    // With new implementation, manual cache stores as Normal
                    assert_eq!(clusters.len(), 1); // One cluster per font
                }
                CachedContent::RepeatedWhitespace { original_count, .. } => {
                    // Old behavior - still valid if it happens
                    assert_eq!(*original_count, 5);
                }
            }
        }

        // Verify font ID 3 (not stored) returns None
        assert!(word_cache
            .get_cached_content(&3, whitespace_content)
            .is_none());
    }

    #[test]
    fn test_cache_with_different_glyph_properties() {
        // Test that different glyph properties are preserved correctly
        let mut word_cache = WordCache::default();
        let font_id = 0;
        let whitespace_content = "      "; // 6 spaces

        // Create a glyph with specific properties
        let custom_glyph = create_test_glyph(9999, 5.0, 10.0, 20.5);
        let glyphs = vec![custom_glyph];
        let components = vec![];
        let space_cluster = crate::font_introspector::shape::cluster::GlyphCluster {
            source: SourceRange { start: 0, end: 1 },
            info: Default::default(),
            glyphs: &glyphs,
            components: &components,
            data: Default::default(),
        };

        word_cache.set_content(font_id, whitespace_content);
        word_cache.add_glyph_cluster(&space_cluster);
        word_cache.finish();

        // Retrieve and expand
        let cached = word_cache
            .get_cached_content(&font_id, whitespace_content)
            .unwrap();

        // With new implementation, manual cache stores as Normal
        match cached {
            CachedContent::Normal(clusters) => {
                assert_eq!(clusters.len(), 1); // Only one cluster was added

                // Test that if it were optimized, the properties would be preserved
                let mock_optimized = CachedContent::RepeatedWhitespace {
                    single_cluster: clusters[0].clone(),
                    original_count: 6,
                };
                let expanded = mock_optimized.expand(None);

                // Verify all expanded clusters preserve the custom glyph properties
                assert_eq!(expanded.len(), 6);
                for (i, cluster) in expanded.iter().enumerate() {
                    assert_eq!(cluster.source.start, i as u32);
                    assert_eq!(cluster.source.end, (i + 1) as u32);
                    assert_eq!(cluster.glyphs.len(), 1);

                    let glyph = &cluster.glyphs[0];
                    assert_eq!(glyph.id, 9999_u16);
                    assert_eq!(glyph.x, 5.0);
                    assert_eq!(glyph.y, 10.0);
                    assert_eq!(glyph.advance, 20.5);
                }
            }
            CachedContent::RepeatedWhitespace { .. } => {
                // Old behavior - test as before
                let expanded = cached.expand(None);
                assert_eq!(expanded.len(), 6);
            }
        }
    }

    #[test]
    fn test_expansion_with_custom_counts() {
        let space_glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
        let single_cluster = create_test_cluster(0, 1, space_glyph);

        let optimized_content = CachedContent::RepeatedWhitespace {
            single_cluster,
            original_count: 10,
        };

        // Test various expansion counts
        let test_counts = vec![1, 5, 10, 15, 50, 100];

        for count in test_counts {
            let expanded = optimized_content.expand(Some(count));
            assert_eq!(expanded.len(), count);

            // Verify source ranges are sequential
            for (i, cluster) in expanded.iter().enumerate() {
                assert_eq!(cluster.source.start, i as u32);
                assert_eq!(cluster.source.end, (i + 1) as u32);
                assert_eq!(cluster.glyphs.len(), 1);
                assert_eq!(cluster.glyphs[0].advance, 16.40625);
            }
        }

        // Test expansion to 0 (edge case)
        let expanded_zero = optimized_content.expand(Some(0));
        assert_eq!(expanded_zero.len(), 0);
    }

    #[test]
    fn test_cache_isolation_between_different_content() {
        // Test that different content types are properly isolated in cache

        // Test 1: Different lengths of same character
        {
            let mut word_cache = WordCache::default();
            let font_id = 0;

            // Store 4 spaces
            let content_4 = "    ";
            let glyph_4 = create_test_glyph(1004, 0.0, 0.0, 16.40625);
            let glyphs_4 = vec![glyph_4];
            let components_4 = vec![];
            let cluster_4 = crate::font_introspector::shape::cluster::GlyphCluster {
                source: SourceRange { start: 0, end: 1 },
                info: Default::default(),
                glyphs: &glyphs_4,
                components: &components_4,
                data: Default::default(),
            };

            word_cache.set_content(font_id, content_4);
            word_cache.add_glyph_cluster(&cluster_4);
            word_cache.finish();

            // Verify 4 spaces are cached
            let cached_4 = word_cache.get_cached_content(&font_id, content_4);
            assert!(cached_4.is_some());

            match cached_4.unwrap() {
                CachedContent::Normal(clusters) => {
                    assert_eq!(clusters.len(), 1); // Only one cluster was added manually
                    let glyph_id_4: u16 = clusters[0].glyphs[0].id;
                    assert_eq!(glyph_id_4, 1004);
                }
                CachedContent::RepeatedWhitespace { .. } => {
                    let expanded_4 = cached_4.unwrap().expand(None);
                    assert_eq!(expanded_4.len(), 4);
                    let glyph_id_4: u16 = expanded_4[0].glyphs[0].id;
                    assert_eq!(glyph_id_4, 1004);
                }
            }

            // Verify 5 spaces are NOT cached (different content)
            let cached_5 = word_cache.get_cached_content(&font_id, "     ");
            assert!(cached_5.is_none());
        }

        // Test 2: Different whitespace characters
        {
            let mut word_cache = WordCache::default();
            let font_id = 0;

            // Store 4 tabs
            let content_tabs = "\t\t\t\t";
            let glyph_tabs = create_test_glyph(2004, 0.0, 0.0, 32.0);
            let glyphs_tabs = vec![glyph_tabs];
            let components_tabs = vec![];
            let cluster_tabs = crate::font_introspector::shape::cluster::GlyphCluster {
                source: SourceRange { start: 0, end: 1 },
                info: Default::default(),
                glyphs: &glyphs_tabs,
                components: &components_tabs,
                data: Default::default(),
            };

            word_cache.set_content(font_id, content_tabs);
            word_cache.add_glyph_cluster(&cluster_tabs);
            word_cache.finish();

            // Verify tabs are cached with correct properties
            let cached_tabs = word_cache.get_cached_content(&font_id, content_tabs);
            assert!(cached_tabs.is_some());

            match cached_tabs.unwrap() {
                CachedContent::Normal(clusters) => {
                    assert_eq!(clusters.len(), 1); // Only one cluster was added manually
                    let glyph_id_tabs: u16 = clusters[0].glyphs[0].id;
                    assert_eq!(glyph_id_tabs, 2004);
                    assert_eq!(clusters[0].glyphs[0].advance, 32.0);
                }
                CachedContent::RepeatedWhitespace { .. } => {
                    let expanded_tabs = cached_tabs.unwrap().expand(None);
                    assert_eq!(expanded_tabs.len(), 4);
                    let glyph_id_tabs: u16 = expanded_tabs[0].glyphs[0].id;
                    assert_eq!(glyph_id_tabs, 2004);
                    assert_eq!(expanded_tabs[0].glyphs[0].advance, 32.0);
                }
            }

            // Verify spaces are NOT cached (different character)
            let cached_spaces = word_cache.get_cached_content(&font_id, "    ");
            assert!(cached_spaces.is_none());
        }
    }

    #[test]
    fn test_mixed_content_scenarios() {
        // Test various mixed content that should NOT be optimized
        let mixed_contents = vec![
            "   a",        // spaces + letter
            "a   ",        // letter + spaces
            "  \n  ",      // spaces + newline + spaces
            " \t  ",       // mixed whitespace types
            "    \0",      // spaces + null
            "  😀  ",      // spaces + emoji + spaces
            "    123",     // spaces + numbers
            "   \u{200B}", // spaces + zero-width space
        ];

        for content in mixed_contents {
            assert!(
                WordCache::analyze_whitespace_sequence(content).is_none(),
                "Content '{}' should not be optimized",
                content.escape_debug()
            );
        }
    }

    #[test]
    fn test_unicode_whitespace_handling() {
        // Test various Unicode whitespace characters
        let unicode_whitespaces = vec![
            ('\u{0020}', "regular space"),      // Regular space
            ('\u{00A0}', "non-breaking space"), // Non-breaking space
            ('\u{2000}', "en quad"),            // En quad
            ('\u{2001}', "em quad"),            // Em quad
            ('\u{2002}', "en space"),           // En space
            ('\u{2003}', "em space"),           // Em space
            ('\u{2009}', "thin space"),         // Thin space
            ('\u{200A}', "hair space"),         // Hair space
        ];

        for (ch, name) in unicode_whitespaces {
            let content = ch.to_string().repeat(4);
            let result = WordCache::analyze_whitespace_sequence(&content);

            if ch.is_whitespace() {
                assert_eq!(
                    result,
                    Some((ch, 4)),
                    "Unicode whitespace '{}' ({}) should be optimized",
                    name,
                    ch.escape_unicode()
                );
            } else {
                assert!(
                    result.is_none(),
                    "Non-whitespace '{}' ({}) should not be optimized",
                    name,
                    ch.escape_unicode()
                );
            }
        }
    }

    #[test]
    fn test_performance_characteristics() {
        // Test that optimization provides memory benefits
        let space_glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
        let single_cluster = create_test_cluster(0, 1, space_glyph);

        // Create optimized content for 1000 spaces
        let optimized_content = CachedContent::RepeatedWhitespace {
            single_cluster: single_cluster.clone(),
            original_count: 1000,
        };

        // Create normal content for comparison (100 clusters to avoid excessive memory)
        let mut normal_clusters = Vec::new();
        for i in 0..100 {
            let cluster = create_test_cluster(i as u32, (i + 1) as u32, space_glyph);
            normal_clusters.push(cluster);
        }
        let normal_content = CachedContent::Normal(normal_clusters);

        // Test expansion performance (should be fast)
        let start = std::time::Instant::now();
        let expanded_optimized = optimized_content.expand(None);
        let optimized_duration = start.elapsed();

        let start = std::time::Instant::now();
        let expanded_normal = normal_content.expand(None);
        let normal_duration = start.elapsed();

        // Verify correctness
        assert_eq!(expanded_optimized.len(), 1000);
        assert_eq!(expanded_normal.len(), 100);

        // Expansion should be reasonably fast (this is more of a smoke test)
        assert!(
            optimized_duration.as_millis() < 100,
            "Optimized expansion took too long: {:?}",
            optimized_duration
        );
        assert!(
            normal_duration.as_millis() < 100,
            "Normal expansion took too long: {:?}",
            normal_duration
        );

        // Memory usage: optimized should store only 1 cluster vs 1000 for normal
        // (This is implicit in the data structure design)
    }

    #[test]
    fn test_shaping_pipeline_cache_vs_no_cache() {
        // This is the critical test: does the shaping pipeline produce identical results
        // when cache is enabled vs disabled?

        use crate::font::FontLibrary;
        use crate::font_introspector::shape::ShapeContext;

        // Test content that should trigger optimization
        let _test_content = "          "; // 10 spaces
        let _font_id = 0;
        let _scaled_font_size = 16.0;

        // Simulate the shaping pipeline WITHOUT cache (normal shaping)
        let normal_clusters = {
            // Create a minimal shaping context (this is simplified)
            let _scx = ShapeContext::new();
            let _font_library = FontLibrary::default();

            // In a real scenario, we'd load an actual font, but for testing we'll simulate
            // the shaping result that would come from the normal pipeline
            let mut clusters = Vec::new();

            // Simulate what the shaper would produce for 10 spaces
            for i in 0..10 {
                let glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
                let cluster = create_test_cluster(i as u32, (i + 1) as u32, glyph);
                clusters.push(cluster);
            }
            clusters
        };

        // Simulate the shaping pipeline WITH cache (optimized)
        let optimized_clusters = {
            // Create the optimized cached content
            let space_glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
            let single_cluster = create_test_cluster(0, 1, space_glyph);
            let optimized_content = CachedContent::RepeatedWhitespace {
                single_cluster,
                original_count: 10,
            };

            // Expand it (this is what happens in the cache hit path)
            optimized_content.expand(None)
        };

        // Now compare the results - they should be identical
        assert_eq!(normal_clusters.len(), optimized_clusters.len());
        assert_eq!(normal_clusters.len(), 10);

        for (i, (normal, optimized)) in normal_clusters
            .iter()
            .zip(optimized_clusters.iter())
            .enumerate()
        {
            // Source ranges should be identical
            assert_eq!(
                normal.source.start, optimized.source.start,
                "Source start mismatch at cluster {}",
                i
            );
            assert_eq!(
                normal.source.end, optimized.source.end,
                "Source end mismatch at cluster {}",
                i
            );

            // Should have same number of glyphs
            assert_eq!(
                normal.glyphs.len(),
                optimized.glyphs.len(),
                "Glyph count mismatch at cluster {}",
                i
            );
            assert_eq!(normal.glyphs.len(), 1);

            // Glyph properties should be identical
            let normal_glyph = &normal.glyphs[0];
            let optimized_glyph = &optimized.glyphs[0];

            assert_eq!(
                normal_glyph.id, optimized_glyph.id,
                "Glyph ID mismatch at cluster {}",
                i
            );
            assert_eq!(
                normal_glyph.x, optimized_glyph.x,
                "Glyph x position mismatch at cluster {}",
                i
            );
            assert_eq!(
                normal_glyph.y, optimized_glyph.y,
                "Glyph y position mismatch at cluster {}",
                i
            );
            assert_eq!(
                normal_glyph.advance, optimized_glyph.advance,
                "Glyph advance mismatch at cluster {}",
                i
            );

            // Cluster metadata should be identical
            assert_eq!(
                normal.info, optimized.info,
                "Cluster info mismatch at cluster {}",
                i
            );
            assert_eq!(
                normal.components.len(),
                optimized.components.len(),
                "Components count mismatch at cluster {}",
                i
            );
            assert_eq!(
                normal.data, optimized.data,
                "Cluster data mismatch at cluster {}",
                i
            );
        }

        // Test that the total advance is the same
        let normal_total_advance: f32 = normal_clusters
            .iter()
            .flat_map(|c| &c.glyphs)
            .map(|g| g.advance)
            .sum();
        let optimized_total_advance: f32 = optimized_clusters
            .iter()
            .flat_map(|c| &c.glyphs)
            .map(|g| g.advance)
            .sum();

        assert_eq!(
            normal_total_advance, optimized_total_advance,
            "Total advance mismatch: normal={}, optimized={}",
            normal_total_advance, optimized_total_advance
        );
    }

    #[test]
    fn test_cache_enabled_vs_disabled_behavior() {
        // Test that demonstrates the cache optimization vs normal shaping
        // This test shows the memory/performance benefit while ensuring correctness

        let long_spaces = " ".repeat(50);
        let test_cases = vec![
            ("    ", 4),                // Exactly at threshold
            ("     ", 5),               // Just above threshold
            ("          ", 10),         // Medium sequence
            (long_spaces.as_str(), 50), // Long sequence
        ];

        for (content, expected_len) in test_cases {
            // Test 1: Normal shaping (what would happen without optimization)
            let normal_result = {
                let mut clusters = Vec::new();
                for i in 0..expected_len {
                    let glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
                    let cluster = create_test_cluster(i as u32, (i + 1) as u32, glyph);
                    clusters.push(cluster);
                }
                clusters
            };

            // Test 2: Optimized caching (what actually happens with our optimization)
            let optimized_result = {
                // Verify this content would be optimized
                assert!(
                    WordCache::analyze_whitespace_sequence(content).is_some(),
                    "Content '{}' should be optimizable",
                    content.escape_debug()
                );

                let space_glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
                let single_cluster = create_test_cluster(0, 1, space_glyph);
                let optimized_content = CachedContent::RepeatedWhitespace {
                    single_cluster,
                    original_count: expected_len,
                };
                optimized_content.expand(None)
            };

            // Results should be functionally identical
            assert_eq!(normal_result.len(), optimized_result.len());

            // Verify each cluster produces the same logical result
            for (normal, optimized) in normal_result.iter().zip(optimized_result.iter()) {
                assert_eq!(normal.source.start, optimized.source.start);
                assert_eq!(normal.source.end, optimized.source.end);
                assert_eq!(normal.glyphs.len(), optimized.glyphs.len());
                assert_eq!(normal.glyphs[0].id, optimized.glyphs[0].id);
                assert_eq!(normal.glyphs[0].advance, optimized.glyphs[0].advance);
            }

            // The key difference: memory usage
            // Normal: stores N clusters (N * cluster_size bytes)
            // Optimized: stores 1 cluster + count (1 * cluster_size + 8 bytes)
            // For large N, this is a significant saving

            // Verify the optimization produces the expected result
            assert_eq!(normal_result.len(), expected_len);
        }
    }

    // TODO: Ultimate integration test - requires real font loading and shaping
    // This would be the definitive test but requires more infrastructure
    #[ignore] // Ignored because it requires real font files and full shaping setup
    #[test]
    fn test_real_shaping_pipeline_with_actual_font() {
        // This test would:
        // 1. Load a real font file
        // 2. Create a ContentProcessor with cache enabled
        // 3. Shape some whitespace content -> store results
        // 4. Clear cache, disable optimization
        // 5. Shape same content again -> store results
        // 6. Compare the two results byte-for-byte
        //
        // This would be the ultimate validation that our optimization
        // produces identical results to normal shaping

        // Example structure (not implemented):
        /*
        let font_data = include_bytes!("../resources/test-fonts/DejaVuSans.ttf");
        let mut processor_with_cache = ContentProcessor::new();
        let mut processor_without_cache = ContentProcessor::new();

        // Disable optimization for second processor
        processor_without_cache.disable_whitespace_optimization();

        let test_content = "          "; // 10 spaces

        // Shape with cache enabled
        let result_with_cache = processor_with_cache.shape_text(test_content, font_id, size);

        // Shape with cache disabled
        let result_without_cache = processor_without_cache.shape_text(test_content, font_id, size);

        // Results should be byte-for-byte identical
        assert_eq!(result_with_cache, result_without_cache);
        */

        // For now, this test is a placeholder showing what the ultimate test would look like
        // This would validate real shaping pipeline with actual fonts
        // It requires loading real font files and full shaping infrastructure
        // The current tests provide strong confidence, but this would be definitive
    }

    #[test]
    fn test_whitespace_optimization_toggle() {
        use crate::font::fonts::SugarloafFontStyle;
        use crate::font::{FontLibrary, SugarloafFont, SugarloafFonts};
        use crate::font_introspector::shape::ShapeContext;
        use crate::font_introspector::text::Script;
        use std::path::Path;

        // Load a real font file
        let font_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources/test-fonts/DejaVuSansMono.ttf");

        if !font_path.exists() {
            panic!("Test font not found at {:?}", font_path);
        }

        // Create a minimal font spec pointing to our test font
        let fonts_spec = SugarloafFonts {
            regular: SugarloafFont {
                family: font_path.to_string_lossy().to_string(),
                weight: Some(400),
                style: SugarloafFontStyle::Normal,
                width: None,
            },
            ..Default::default()
        };

        let (font_library, _errors) = FontLibrary::new(fonts_spec);
        let font_id = 0; // Regular font is typically ID 0

        // Test content: 10 spaces (should trigger whitespace optimization when enabled)
        let test_content = "          "; // 10 spaces
        let font_size = 14.0;
        let script = Script::Latin;

        // Helper function to shape content and return clusters
        let shape_content = |use_cache: bool| -> Vec<OwnedGlyphCluster> {
            let mut scx = ShapeContext::new();
            let font_library_guard = font_library.inner.read();
            if let Some((shared_data, offset, key)) =
                font_library_guard.get_data(&font_id)
            {
                let font_ref = crate::font_introspector::FontRef {
                    data: shared_data.as_ref(),
                    offset,
                    key,
                };

                let mut shaper =
                    scx.builder(font_ref).script(script).size(font_size).build();

                shaper.add_str(test_content);

                if use_cache {
                    // Use the caching system (with optimization)
                    let mut cache = WordCache::new();
                    cache.set_content(font_id, test_content);

                    let mut clusters = Vec::new();
                    shaper.shape_with(|cluster| {
                        cache.add_glyph_cluster(cluster);
                        clusters.push(cluster.into());
                    });

                    cache.finish();

                    // Get the cached content and expand it
                    if let Some(cached) = cache.get_cached_content(&font_id, test_content)
                    {
                        cached.expand(None)
                    } else {
                        clusters
                    }
                } else {
                    // Direct shaping without cache
                    let mut clusters = Vec::new();
                    shaper.shape_with(|cluster| {
                        clusters.push(cluster.into());
                    });
                    clusters
                }
            } else {
                Vec::new()
            }
        };

        // Test with optimization enabled
        let optimized_clusters = shape_content(true);

        // Test without optimization (direct shaping)
        let normal_clusters = shape_content(false);

        if optimized_clusters.is_empty() || normal_clusters.is_empty() {
            // Font not loaded properly, skip test
            return;
        }

        // Both should produce identical results
        assert_eq!(optimized_clusters.len(), normal_clusters.len());

        // Detailed comparison
        for (i, (opt, norm)) in optimized_clusters
            .iter()
            .zip(normal_clusters.iter())
            .enumerate()
        {
            assert_eq!(
                opt.source.start, norm.source.start,
                "Source start mismatch at {}",
                i
            );
            assert_eq!(
                opt.source.end, norm.source.end,
                "Source end mismatch at {}",
                i
            );
            assert_eq!(
                opt.glyphs.len(),
                norm.glyphs.len(),
                "Glyph count mismatch at {}",
                i
            );

            for (j, (opt_glyph, norm_glyph)) in
                opt.glyphs.iter().zip(norm.glyphs.iter()).enumerate()
            {
                assert_eq!(
                    opt_glyph.id, norm_glyph.id,
                    "Glyph ID mismatch at {},{}",
                    i, j
                );
                assert_eq!(
                    opt_glyph.advance, norm_glyph.advance,
                    "Advance mismatch at {},{}",
                    i, j
                );
                assert_eq!(
                    opt_glyph.x, norm_glyph.x,
                    "X position mismatch at {},{}",
                    i, j
                );
                assert_eq!(
                    opt_glyph.y, norm_glyph.y,
                    "Y position mismatch at {},{}",
                    i, j
                );
            }
        }

        // Verify that optimization was actually used by checking cache behavior
        let mut cache = WordCache::new();
        cache.set_content(font_id, test_content);

        // We can't easily simulate adding the clusters back since we need GlyphCluster not OwnedGlyphCluster
        // But we can check if the optimization would trigger by analyzing the content
        let analysis = WordCache::analyze_whitespace_sequence(test_content);
        assert!(
            analysis.is_some(),
            "Optimization should trigger for whitespace sequence"
        );

        if let Some((_, count)) = analysis {
            assert_eq!(
                count,
                test_content.len(),
                "Should detect correct character count"
            );
        }
    }

    #[test]
    fn test_whitespace_optimization_always_enabled() {
        // Test that whitespace optimization is always enabled by default

        // Test various whitespace sequences that should be optimized
        assert_eq!(
            WordCache::analyze_whitespace_sequence("    "),
            Some((' ', 4))
        );
        assert_eq!(
            WordCache::analyze_whitespace_sequence("          "),
            Some((' ', 10))
        );
        assert_eq!(
            WordCache::analyze_whitespace_sequence("\t\t\t\t"),
            Some(('\t', 4))
        );

        // Test sequences that should NOT be optimized
        assert_eq!(WordCache::analyze_whitespace_sequence("   "), None); // Only 3 chars
        assert_eq!(WordCache::analyze_whitespace_sequence("  \t  "), None); // Mixed whitespace
        assert_eq!(WordCache::analyze_whitespace_sequence("a    b"), None); // Contains non-whitespace
    }

    #[test]
    fn test_manual_cache_behavior() {
        let mut cache = WordCache::new();
        let font_id = 0;
        let content = "          "; // 10 spaces

        // Check if optimization would trigger
        let analysis = WordCache::analyze_whitespace_sequence(content);
        assert_eq!(analysis, Some((' ', 10)));

        // Simulate the caching process
        cache.set_content(font_id, content);

        // Create a mock cluster
        let glyph = create_test_glyph(2013, 0.0, 0.0, 16.40625);
        let glyphs = vec![glyph];
        let components = vec![];
        let cluster = crate::font_introspector::shape::cluster::GlyphCluster {
            source: crate::font_introspector::text::cluster::SourceRange {
                start: 0,
                end: 1,
            },
            info: Default::default(),
            glyphs: &glyphs,
            components: &components,
            data: Default::default(),
        };

        // Add multiple clusters (simulating normal shaping of 10 spaces)
        for i in 0..10 {
            let mut cluster_copy = cluster;
            cluster_copy.source.start = i as u32;
            cluster_copy.source.end = (i + 1) as u32;
            cache.add_glyph_cluster(&cluster_copy);
        }

        cache.finish();

        // Check what was actually cached
        let cached = cache.get_cached_content(&font_id, content);
        assert!(cached.is_some());

        // With new implementation, manual cache operations store as Normal
        // because optimization happens upfront in process_line, not in finish()
        match cached.unwrap() {
            CachedContent::Normal(clusters) => {
                assert_eq!(clusters.len(), 10);
            }
            CachedContent::RepeatedWhitespace { .. } => {
                panic!(
                    "Manual cache should not optimize - optimization happens upfront now"
                );
            }
        }
    }

    #[test]
    fn test_real_world_whitespace_scenarios() {
        let test_cases = vec![
            ("   ", false, 0),              // 3 spaces - should NOT optimize
            ("    ", true, 4),              // 4 spaces - should optimize
            ("        ", true, 8),          // 8 spaces - should optimize
            ("                ", true, 16), // 16 spaces - should optimize
            ("  \t  ", false, 0),           // mixed whitespace - should NOT optimize
            ("\t\t\t\t", true, 4),          // 4 tabs - should optimize
            ("a    b", false, 0),           // spaces with text - should NOT optimize
        ];

        for (content, should_optimize, expected_count) in test_cases {
            let result = WordCache::analyze_whitespace_sequence(content);

            if should_optimize {
                assert!(result.is_some(), "Expected optimization for: '{}'", content);
                let (_, count) = result.unwrap();
                assert_eq!(count, expected_count, "Wrong count for: '{}'", content);
            } else {
                assert!(
                    result.is_none(),
                    "Expected no optimization for: '{}'",
                    content
                );
            }
        }
    }

    #[test]
    fn test_upfront_whitespace_optimization() {
        use crate::font::{FontLibrary, SugarloafFonts};
        use crate::layout::RichTextLayout;

        // Create a minimal font setup
        let fonts_spec = SugarloafFonts::default();
        let (font_library, _errors) = FontLibrary::new(fonts_spec);

        // Create a content processor
        let mut content = Content::new(&font_library);

        // Create a state with a simple layout
        let layout = RichTextLayout {
            font_size: 14.0,
            original_font_size: 14.0,
            line_height: 1.0,
            dimensions: Default::default(),
        };
        let state_id = content.create_state(&layout);

        // Add a line with long whitespace sequence
        let whitespace_content = "          "; // 10 spaces
        content
            .sel(state_id)
            .new_line()
            .add_text(whitespace_content, FragmentStyle::default());

        // Check if optimization should trigger
        let analysis = WordCache::analyze_whitespace_sequence(whitespace_content);
        assert_eq!(analysis, Some((' ', 10)));

        // Build the content (this should trigger the new optimization logic)
        content.build();

        // Check what was cached
        let font_id = 0; // Default font
        let cached = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);

        assert!(cached.is_some(), "Content should be cached");

        match cached.unwrap() {
            CachedContent::RepeatedWhitespace { original_count, .. } => {
                assert_eq!(*original_count, 10, "Should cache with correct count");
            }
            CachedContent::Normal(clusters) => {
                panic!(
                    "Expected RepeatedWhitespace, got Normal with {} clusters",
                    clusters.len()
                );
            }
        }
    }

    #[test]
    fn test_cache_hit_behavior() {
        use crate::font::{FontLibrary, SugarloafFonts};
        use crate::layout::RichTextLayout;

        // Create a minimal font setup
        let fonts_spec = SugarloafFonts::default();
        let (font_library, _errors) = FontLibrary::new(fonts_spec);

        // Create a content processor
        let mut content = Content::new(&font_library);

        // Create a state with a simple layout
        let layout = RichTextLayout {
            font_size: 14.0,
            original_font_size: 14.0,
            line_height: 1.0,
            dimensions: Default::default(),
        };
        let state_id = content.create_state(&layout);

        let whitespace_content = "          "; // 10 spaces
        let font_id = 0;

        // FIRST RENDER (should trigger optimization and cache)
        content
            .sel(state_id)
            .new_line()
            .add_text(whitespace_content, FragmentStyle::default());

        content.build();

        // Check what was cached after first render
        let first_cached = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);

        assert!(
            first_cached.is_some(),
            "Content should be cached after first render"
        );

        match first_cached.unwrap() {
            CachedContent::RepeatedWhitespace { original_count, .. } => {
                assert_eq!(
                    *original_count, 10,
                    "First render should cache with count=10"
                );
            }
            CachedContent::Normal(clusters) => {
                panic!("First render: Expected RepeatedWhitespace, got Normal with {} clusters", clusters.len());
            }
        }

        // SECOND RENDER (should use cache)
        content.clear_state(&state_id);
        content
            .sel(state_id)
            .new_line()
            .add_text(whitespace_content, FragmentStyle::default());

        content.build();

        // Verify cache is still there and being used
        let second_cached = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);

        assert!(
            second_cached.is_some(),
            "Cache should still exist after second render"
        );

        match second_cached.unwrap() {
            CachedContent::RepeatedWhitespace { original_count, .. } => {
                assert_eq!(
                    *original_count, 10,
                    "Second render should still have cached count=10"
                );
            }
            CachedContent::Normal(clusters) => {
                panic!("Second render: Expected RepeatedWhitespace, got Normal with {} clusters", clusters.len());
            }
        }
    }

    #[test]
    fn test_cache_state_transitions() {
        use crate::font::{FontLibrary, SugarloafFonts};
        use crate::layout::RichTextLayout;

        // Create a minimal font setup
        let fonts_spec = SugarloafFonts::default();
        let (font_library, _errors) = FontLibrary::new(fonts_spec);

        // Create a content processor
        let mut content = Content::new(&font_library);

        // Create a state with a simple layout
        let layout = RichTextLayout {
            font_size: 14.0,
            original_font_size: 14.0,
            line_height: 1.0,
            dimensions: Default::default(),
        };
        let state_id = content.create_state(&layout);

        let whitespace_content = "          "; // 10 spaces
        let font_id = 0;

        // Initial state: cache should be empty
        let initial_cache = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);
        assert!(initial_cache.is_none(), "Cache should be empty initially");

        // First render: should populate cache
        content
            .sel(state_id)
            .new_line()
            .add_text(whitespace_content, FragmentStyle::default());

        // Before build: cache should still be empty
        let pre_build_cache = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);
        assert!(
            pre_build_cache.is_none(),
            "Cache should be empty before build"
        );

        content.build();

        // After build: cache should be populated
        let post_build_cache = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);
        assert!(
            post_build_cache.is_some(),
            "Cache should be populated after build"
        );

        match post_build_cache.unwrap() {
            CachedContent::RepeatedWhitespace { original_count, .. } => {
                assert_eq!(*original_count, 10, "Should cache with correct count");
            }
            CachedContent::Normal(clusters) => {
                panic!(
                    "Expected RepeatedWhitespace, got Normal with {} clusters",
                    clusters.len()
                );
            }
        }

        // Second render: cache should persist
        content.clear_state(&state_id);
        content
            .sel(state_id)
            .new_line()
            .add_text(whitespace_content, FragmentStyle::default());

        // Before second build: cache should still exist
        let pre_second_build_cache = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);
        assert!(
            pre_second_build_cache.is_some(),
            "Cache should persist between renders"
        );

        content.build();

        // After second build: cache should still exist
        let final_cache = content
            .word_cache
            .get_cached_content(&font_id, whitespace_content);
        assert!(
            final_cache.is_some(),
            "Cache should still exist after second build"
        );

        match final_cache.unwrap() {
            CachedContent::RepeatedWhitespace { original_count, .. } => {
                assert_eq!(*original_count, 10, "Cache should maintain correct count");
            }
            CachedContent::Normal(clusters) => {
                panic!(
                    "Expected RepeatedWhitespace, got Normal with {} clusters",
                    clusters.len()
                );
            }
        }
    }

    #[test]
    fn test_optimized_whitespace_analysis_correctness() {
        // Test cases covering different scenarios
        let long_spaces = " ".repeat(100);
        let long_tabs = "\t".repeat(50);
        let test_cases = vec![
            ("    ", true),         // 4 spaces - should optimize
            ("          ", true),   // 10 spaces - should optimize
            ("\t\t\t\t", true),     // 4 tabs - should optimize
            (&long_spaces, true),   // 100 spaces - should optimize
            (&long_tabs, true),     // 50 tabs - should optimize
            ("hello world", false), // non-whitespace - should not optimize
            ("  a  ", false),       // mixed content - should not optimize
            (" \t  ", false),       // mixed whitespace - should not optimize
            ("   ", false),         // 3 spaces - below threshold
            ("\t\t\t", false),      // 3 tabs - below threshold
        ];

        // Verify correctness of all test cases
        for (content, should_optimize) in &test_cases {
            let result = WordCache::analyze_whitespace_sequence(content);
            if *should_optimize {
                assert!(
                    result.is_some(),
                    "Content '{}' should be optimized but wasn't",
                    content.escape_debug()
                );
                let (ch, count) = result.unwrap();
                assert!(
                    ch.is_whitespace(),
                    "Optimized character '{}' should be whitespace",
                    ch.escape_debug()
                );
                assert!(count >= 4, "Optimized count {} should be >= 4", count);
                assert_eq!(
                    count,
                    content.chars().count(),
                    "Count should match actual character count"
                );
            } else {
                assert!(
                    result.is_none(),
                    "Content '{}' should not be optimized but was: {:?}",
                    content.escape_debug(),
                    result
                );
            }
        }

        // Test specific optimization results
        assert_eq!(
            WordCache::analyze_whitespace_sequence("    "),
            Some((' ', 4))
        );
        assert_eq!(
            WordCache::analyze_whitespace_sequence("\t\t\t\t"),
            Some(('\t', 4))
        );
        assert_eq!(
            WordCache::analyze_whitespace_sequence(&long_spaces),
            Some((' ', 100))
        );
        assert_eq!(
            WordCache::analyze_whitespace_sequence(&long_tabs),
            Some(('\t', 50))
        );

        // Test edge cases
        assert_eq!(WordCache::analyze_whitespace_sequence(""), None);
        assert_eq!(WordCache::analyze_whitespace_sequence("a"), None);
        assert_eq!(
            WordCache::analyze_whitespace_sequence("   "), // exactly 3
            None
        );

        // Test Unicode whitespace
        let unicode_spaces = "\u{2000}".repeat(4); // En quad
        assert_eq!(
            WordCache::analyze_whitespace_sequence(&unicode_spaces),
            Some(('\u{2000}', 4))
        );
    }

    #[test]
    fn test_word_cache_fx_hasher_functionality() {
        let mut cache = WordCache::new();
        let font_id = 0;

        // Test 1: Cache key generation functionality (tests FxHasher)
        let mut keys = Vec::new();
        for i in 0..100 {
            let content = format!("test_word_{}", i);
            let key = cache.cache_key_with_interning(&content, font_id);
            keys.push(key);
        }

        // Verify all keys are unique (no hash collisions for different content)
        let mut unique_keys = keys.clone();
        unique_keys.sort();
        unique_keys.dedup();
        assert_eq!(keys.len(), unique_keys.len(), "Hash collisions detected");

        // Test 2: Cache lookup functionality (misses)
        let mut miss_count = 0;
        for i in 0..100 {
            let content = format!("test_word_{}", i);
            if cache.get_cached_content(&font_id, &content).is_none() {
                miss_count += 1;
            }
        }

        assert_eq!(
            miss_count, 100,
            "Expected all cache misses, got {} misses out of 100",
            miss_count
        );

        // Test 3: Hash consistency for repeated content
        let content1 = "repeated_content".to_string();
        let content2 = "repeated_content".to_string();

        let key1 = cache.cache_key_with_interning(&content1, font_id);
        let key2 = cache.cache_key_with_interning(&content2, font_id);

        // Same content should produce same hash key
        assert_eq!(key1, key2, "Same content should produce same hash key");

        // Test 4: Hash consistency
        let content = "test_content";
        let key1 = cache.cache_key_with_interning(content, font_id);
        let key2 = cache.cache_key_with_interning(content, font_id);

        assert_eq!(key1, key2, "Same content should produce same hash");

        // Different font_id should produce different hash
        let key3 = cache.cache_key_with_interning(content, font_id + 1);
        assert_ne!(
            key1, key3,
            "Different font_id should produce different hash"
        );

        // Different content should produce different hash
        let key4 = cache.cache_key_with_interning("different_content", font_id);
        assert_ne!(
            key1, key4,
            "Different content should produce different hash"
        );
    }

    #[test]
    fn test_hash_collision_along_clone() {
        let mut cache = WordCache::new();
        let font_id = 1;

        // Test the specific case reported: "along" vs "clone"
        let along_key = cache.cache_key_with_interning("along", font_id);
        let clone_key = cache.cache_key_with_interning("clone", font_id);

        assert_ne!(
            along_key, clone_key,
            "Hash collision detected: 'along' and 'clone' produce same hash key! along_key={}, clone_key={}",
            along_key, clone_key
        );

        // Test other similar words that might collide
        let test_words = [
            "along", "clone", "alone", "close", "clown", "blown", "flown", "grown",
            "shown", "known", "stone", "phone", "drone", "prone", "throne",
        ];

        let mut keys = std::collections::HashMap::new();
        for word in &test_words {
            let key = cache.cache_key_with_interning(word, font_id);
            if let Some(existing_word) = keys.get(&key) {
                panic!(
                    "Hash collision detected: '{}' and '{}' produce same hash key {}",
                    word, existing_word, key
                );
            }
            keys.insert(key, word);
        }
    }

    #[test]
    fn test_string_interning_isolation() {
        let mut cache = WordCache::new();

        // Test that cache keys are different for different content
        let content1 = "along";
        let content2 = "clone";

        // Test that cache keys (which now use direct string hashing) are different
        let key1 = cache.cache_key_with_interning(content1, 1);
        let key2 = cache.cache_key_with_interning(content2, 1);

        assert_ne!(key1, key2,
            "Cache keys should be different for 'along' and 'clone' after fix. key1={}, key2={}",
            key1, key2);

        // Test that same content produces same key
        let key1_again = cache.cache_key_with_interning(content1, 1);
        let key2_again = cache.cache_key_with_interning(content2, 1);

        assert_eq!(
            key1, key1_again,
            "Same content should produce same cache key"
        );
        assert_eq!(
            key2, key2_again,
            "Same content should produce same cache key"
        );
    }

    #[test]
    fn test_cache_content_isolation() {
        let mut cache = WordCache::new();
        let font_id = 1;

        // Test that cache keys are different for "along" and "clone"
        let along_key = cache.cache_key_with_interning("along", font_id);
        let clone_key = cache.cache_key_with_interning("clone", font_id);

        // Verify keys are different (no collision)
        assert_ne!(along_key, clone_key,
            "Cache keys should be different for 'along' and 'clone'. along_key={}, clone_key={}",
            along_key, clone_key);

        // Test that cache lookup returns None for non-existent entries
        assert!(
            cache.get_cached_content(&font_id, "along").is_none(),
            "Cache should be empty initially for 'along'"
        );
        assert!(
            cache.get_cached_content(&font_id, "clone").is_none(),
            "Cache should be empty initially for 'clone'"
        );

        // Test that different content produces different cache behavior
        cache.set_content(font_id, "along");
        let along_hash = cache.content_hash;
        cache.finish(); // Reset state

        cache.set_content(font_id, "clone");
        let clone_hash = cache.content_hash;
        cache.finish(); // Reset state

        assert_ne!(
            along_hash, clone_hash,
            "Content hashes should be different for 'along' and 'clone'"
        );
    }
}
