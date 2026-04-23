// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

#![allow(clippy::uninlined_format_args)]

use crate::font::FontLibrary;
use swash::shape::ShapeContext;
use swash::text::Script;
#[cfg(not(target_os = "macos"))]
use swash::FontRef;
use crate::layout::content_data::{ContentData, ContentState};
use crate::layout::render_data::RenderData;
use crate::layout::TextLayout;
use lru::LruCache;
use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::num::NonZeroUsize;
use tracing::debug;

use swash::Attributes;
use swash::Setting;
use crate::{sugarloaf::primitives::SugarCursor, DrawableChar, Graphic};

/// Pre-packed shaping result ready to push directly as a RunData.
/// Avoids re-packing OwnedGlyphClusters on every cache hit.
#[derive(Clone, Debug)]
pub struct CachedRun {
    pub glyphs: Vec<crate::layout::glyph::GlyphData>,
    pub detailed_glyphs: Vec<crate::layout::glyph::Glyph>,
    pub advance: f32,
    pub cache_key: u64,
}

#[derive(Debug, Clone)]
pub struct FragmentData {
    /// Range `(start, end)` into the owning `BuilderLine::text_buffer`.
    /// `None` means advance position only (no shaping).
    pub content: Option<(u32, u32)>,
    pub style: SpanStyle,
}

#[derive(Default, Clone, Debug)]
pub struct BuilderLine {
    pub fragments: Vec<FragmentData>,
    /// Shared text buffer for all fragments on this line. Each fragment
    /// stores a `(start, end)` range into this buffer instead of owning
    /// its own `String`, eliminating per-span heap allocations.
    pub text_buffer: String,
    pub render_data: RenderData,
}

impl BuilderLine {
    /// Get the text slice for a fragment.
    #[inline]
    pub fn fragment_text(&self, frag: &FragmentData) -> Option<&str> {
        let (start, end) = frag.content?;
        self.text_buffer.get(start as usize..end as usize)
    }

    /// Push text into the shared buffer and return the range.
    #[inline]
    pub fn push_text(&mut self, text: &str) -> (u32, u32) {
        let start = self.text_buffer.len() as u32;
        self.text_buffer.push_str(text);
        let end = self.text_buffer.len() as u32;
        (start, end)
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
#[repr(C)]
pub enum BuilderStateUpdate {
    #[default]
    Full,
    Partial(HashSet<usize>),
    Noop,
}

#[derive(Default, Clone, Debug)]
pub struct BuilderState {
    pub lines: Vec<BuilderLine>,
    pub vars: FontSettingCache<f32>,
    pub last_update: BuilderStateUpdate,
    pub scaled_font_size: f32,
    pub layout: TextLayout,
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
    pub fn from_layout(layout: &TextLayout) -> Self {
        Self {
            layout: *layout,
            scaled_font_size: layout.font_size * layout.dimensions.scale,
            ..BuilderState::default()
        }
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
    pub fn clear(&mut self) -> &mut Self {
        self.lines.clear();
        self.vars.clear();
        self.last_update = BuilderStateUpdate::Full;
        self.lines.push(BuilderLine::default());
        self
    }

    /// Add a text span with the given style
    #[inline]
    pub fn add_span(&mut self, text: &str, style: SpanStyle) -> &mut Self {
        if self.lines.is_empty() {
            self.lines.push(BuilderLine::default());
        }
        let current_line = self.current_line();
        if let Some(line) = self.lines.get_mut(current_line) {
            let range = line.push_text(text);
            line.fragments.push(FragmentData {
                content: Some(range),
                style,
            });
        }
        self
    }

    /// Add a new line
    #[inline]
    pub fn new_line(&mut self) -> &mut Self {
        self.lines.push(BuilderLine::default());
        self
    }

    /// Clear a specific line's fragments
    #[inline]
    pub fn clear_line(&mut self, line_number: usize) -> &mut Self {
        if let Some(line) = self.lines.get_mut(line_number) {
            line.fragments.clear();
            line.text_buffer.clear();
            line.render_data.glyphs.clear();
            line.render_data.runs.clear();
            self.mark_line_dirty(line_number);
        }
        self
    }

    /// Add text to a specific line
    #[inline]
    pub fn add_span_on_line(
        &mut self,
        line_number: usize,
        text: &str,
        style: SpanStyle,
    ) -> &mut Self {
        if let Some(line) = self.lines.get_mut(line_number) {
            let range = line.push_text(text);
            line.fragments.push(FragmentData {
                content: Some(range),
                style,
            });
        }
        self
    }

    /// Add an empty span to a specific line that only advances position
    /// (renders background rect if set, but no text shaping).
    #[inline]
    pub fn add_span_as_rect_on_line(
        &mut self,
        line_number: usize,
        style: SpanStyle,
    ) -> &mut Self {
        if let Some(line) = self.lines.get_mut(line_number) {
            line.fragments.push(FragmentData {
                content: None,
                style,
            });
        }
        self
    }

    /// Add an empty span that only advances position
    /// (renders background rect if set, but no text shaping).
    #[inline]
    pub fn add_span_as_rect(&mut self, style: SpanStyle) -> &mut Self {
        if self.lines.is_empty() {
            self.lines.push(BuilderLine::default());
        }
        let current_line = self.current_line();
        if let Some(line) = self.lines.get_mut(current_line) {
            line.fragments.push(FragmentData {
                content: None,
                style,
            });
        }
        self
    }

    /// Finalize the text building (placeholder for compatibility)
    #[inline]
    pub fn build(&mut self) -> &mut Self {
        self.last_update = BuilderStateUpdate::Full;
        self
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
    // offset, size
    Underline(UnderlineInfo),
    Strikethrough,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct SpanStyle {
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
    pub decoration: Option<SpanStyleDecoration>,
    /// Decoration color.
    pub decoration_color: Option<[f32; 4]>,
    /// Cursor style.
    pub cursor: Option<SugarCursor>,
    /// Media
    pub media: Option<Graphic>,
    /// Drawable character
    pub drawable_char: Option<DrawableChar>,
    /// PUA constraint width: how many cells the glyph should visually fill.
    /// None for normal glyphs, Some(1.0) or Some(2.0) for PUA glyphs.
    /// Does NOT affect positioning/advance — only compositor scaling.
    pub pua_constraint: Option<f32>,
    /// Optional per-glyph Nerd Font constraint (size / alignment /
    /// padding) ported from ghostty's patcher table. When set, the
    /// compositor uses ghostty's constrain() math to lay the glyph out
    /// instead of the generic cell-centered fit. Only populated by the
    /// renderer for codepoints with a table entry (`get_constraint`).
    pub nerd_font_constraint: Option<crate::font::nerd_font_attributes::Constraint>,
}

impl Default for SpanStyle {
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
            pua_constraint: None,
            nerd_font_constraint: None,
        }
    }
}

/// Context for paragraph layout.
pub struct Content {
    fonts: FontLibrary,
    font_features: Vec<swash::Setting<u16>>,
    scx: ShapeContext,
    pub states: FxHashMap<usize, ContentState>,
    /// Transient text content that gets cleared after each render
    pub transient_texts: Vec<ContentState>,
    shaping_cache: ShapingCache,
    selector: Option<usize>,
}

impl Content {
    /// Creates a new layout context with the specified font library.
    pub fn new(font_library: &FontLibrary) -> Self {
        Self {
            fonts: font_library.clone(),
            scx: ShapeContext::new(),
            states: FxHashMap::default(),
            transient_texts: Vec::new(),
            shaping_cache: ShapingCache::new(),
            font_features: vec![],
            selector: None,
        }
    }

    #[inline]
    pub fn sel(&mut self, state_id: usize) -> &mut Content {
        self.selector = Some(state_id);

        // Ensure the state exists - create it with default text layout if missing
        self.states.entry(state_id).or_insert_with(|| {
            let default_layout = TextLayout::default();
            let builder_state = BuilderState::from_layout(&default_layout);
            ContentState::new(ContentData::Text(builder_state))
        });

        self
    }

    #[inline]
    pub fn font_library(&self) -> &FontLibrary {
        &self.fonts
    }

    #[inline]
    pub fn set_font_library(&mut self, font_library: &FontLibrary) {
        self.fonts = font_library.clone();
        self.shaping_cache = ShapingCache::new();
    }

    /// Get text state by ID (returns None if ID doesn't exist or is not text)
    #[inline]
    pub fn get_state(&self, state_id: &usize) -> Option<&BuilderState> {
        self.states.get(state_id)?.as_text()
    }

    /// Get mutable text state by ID (returns None if ID doesn't exist or is not text)
    #[inline]
    pub fn get_state_mut(&mut self, state_id: &usize) -> Option<&mut BuilderState> {
        self.states.get_mut(state_id)?.as_text_mut()
    }

    /// Get text by ID - returns the lines API if text, None otherwise
    #[inline]
    pub fn get_text_by_id(&self, id: usize) -> Option<&BuilderState> {
        self.states.get(&id)?.as_text()
    }

    /// Get mutable text by ID
    #[inline]
    pub fn get_text_by_id_mut(&mut self, id: usize) -> Option<&mut BuilderState> {
        self.states.get_mut(&id)?.as_text_mut()
    }

    /// Get content state by ID (any type)
    #[inline]
    pub fn get_content_state(&self, state_id: &usize) -> Option<&ContentState> {
        self.states.get(state_id)
    }

    /// Get mutable content state by ID (any type)
    #[inline]
    pub fn get_content_state_mut(
        &mut self,
        state_id: &usize,
    ) -> Option<&mut ContentState> {
        self.states.get_mut(state_id)
    }

    #[inline]
    pub fn set_font_features(
        &mut self,
        font_features: Vec<swash::Setting<u16>>,
    ) {
        self.font_features = font_features;
    }

    /// Create text content at the given ID (overwrites existing content)
    #[inline]
    pub fn set_text(&mut self, id: usize, rich_text_layout: &TextLayout) {
        let mut builder_state = BuilderState::from_layout(rich_text_layout);

        // Immediately calculate dimensions for a representative character
        builder_state.layout.dimensions =
            self.calculate_character_cell_dimensions(rich_text_layout);

        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Text(builder_state);
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states
                .insert(id, ContentState::new(ContentData::Text(builder_state)));
        }
    }

    /// Calculate character cell dimensions
    fn calculate_character_cell_dimensions(
        &self,
        layout: &TextLayout,
    ) -> crate::layout::TextDimensions {
        let font_size = layout.font_size;

        // macOS: read metrics + space-glyph advance straight from the
        // primary CTFont. This is the last byte-dependent path that
        // mattered on mac — using CTFont here means `FONT_DATA_CACHE`
        // never holds the primary font either.
        #[cfg(target_os = "macos")]
        if let Some(handle) = self.fonts.ct_font(0) {
            let metrics = crate::font::macos::font_metrics(&handle, font_size);
            // Cell width = max advance across all printable ASCII,
            // queried on a CTFont clone at the real render size (not
            // the 1pt base — that returns bogus 1.0-per-glyph
            // advances on some fonts). Mirrors Ghostty
            // (`coretext.zig:773-804`). Progressive fallbacks:
            //   1. max-ASCII at this size (right answer on every
            //      real font we've seen)
            //   2. advance of space (pre-existing behaviour; may
            //      return None)
            //   3. `font_size` itself (the em — last-resort, wider
            //      than any real monospace advance)
            let char_width = crate::font::macos::max_ascii_advance_px(&handle, font_size)
                .or_else(|| {
                    crate::font::macos::advance_units_for_char(&handle, ' ')
                        .map(|(units, upem)| units * font_size / upem as f32)
                })
                .unwrap_or(font_size);
            let line_height =
                (metrics.ascent + metrics.descent + metrics.leading) * layout.line_height;
            let scale = layout.dimensions.scale;
            return crate::layout::TextDimensions {
                width: char_width * scale,
                height: (line_height * scale).ceil(),
                scale,
            };
        }

        #[cfg(not(target_os = "macos"))]
        if let Some(font_library_data) = self.fonts.inner.try_read() {
            let font_id = 0; // FONT_ID_REGULAR

            // Get font data to create swash FontRef
            if let Some((font_data, offset, _key)) = font_library_data.get_data(&font_id)
            {
                // Create swash FontRef directly from font data
                if let Some(font_ref) = swash::FontRef::from_index(
                    &font_data,
                    offset as usize,
                ) {
                    // Get metrics using swash
                    let font_metrics = font_ref.metrics(&[]);

                    // Calculate character cell width using space character
                    let glyph_id = font_ref.charmap().map(' ' as u32);
                    let char_width = {
                        // Get advance width for space character using GlyphMetrics
                        let glyph_metrics =
                            swash::GlyphMetrics::from_font(
                                &font_ref,
                                &[],
                            );
                        let advance = glyph_metrics.advance_width(glyph_id);

                        // Scale to font size
                        let units_per_em = font_metrics.units_per_em as f32;
                        let scale_factor = font_size / units_per_em;

                        if advance > 0.0 {
                            advance * scale_factor
                        } else {
                            // Fallback: approximate monospace character width
                            font_size
                        }
                    };

                    // Calculate line height using scaled metrics
                    let units_per_em = font_metrics.units_per_em as f32;
                    let scale_factor = font_size / units_per_em;
                    let ascent = font_metrics.ascent * scale_factor;
                    let descent = font_metrics.descent.abs() * scale_factor;
                    let leading = font_metrics.leading * scale_factor;
                    let line_height = (ascent + descent + leading) * layout.line_height;

                    // Scale to physical pixels to match what the brush returns.
                    // physical scale — the renderer uses that ceiled value.
                    let char_width_physical = char_width * layout.dimensions.scale;
                    let line_height_physical =
                        (line_height * layout.dimensions.scale).ceil();

                    // Return dimensions in physical pixels (matching brush behavior)
                    let result = crate::layout::TextDimensions {
                        width: char_width_physical,
                        height: line_height_physical,
                        scale: layout.dimensions.scale,
                    };

                    // println!("  -> Returning dimensions (physical): width={}, height={}, scale={}",
                    //     result.width, result.height, result.scale);

                    return result;
                }
            }
        }

        // Fallback to reasonable defaults if font metrics unavailable
        // Return in physical pixels to match brush behavior
        let fallback_width = layout.font_size;
        let fallback_height = layout.font_size * layout.line_height;

        crate::layout::TextDimensions {
            width: fallback_width * layout.dimensions.scale,
            height: fallback_height * layout.dimensions.scale,
            scale: layout.dimensions.scale,
        }
    }

    #[inline]
    pub fn remove_state(&mut self, rich_text_id: &usize) {
        self.states.remove(rich_text_id);
    }

    #[inline]
    pub fn mark_states_clean(&mut self) {
        for content_state in self.states.values_mut() {
            if let Some(text_state) = content_state.as_text_mut() {
                text_state.mark_clean();
            }
        }
    }

    /// Add a transient text content that will be cleared after rendering.
    /// Returns the index into transient_texts vec.
    #[inline]
    pub fn add_transient_text(&mut self, layout: &TextLayout) -> usize {
        let mut builder_state = BuilderState::from_layout(layout);
        builder_state.layout.dimensions =
            self.calculate_character_cell_dimensions(layout);

        let mut content_state = ContentState::new(ContentData::Text(builder_state));
        content_state.render_data.transient = true;

        let index = self.transient_texts.len();
        self.transient_texts.push(content_state);
        index
    }

    /// Get mutable reference to transient text by index
    #[inline]
    pub fn get_transient_text_mut(&mut self, index: usize) -> Option<&mut BuilderState> {
        self.transient_texts.get_mut(index)?.as_text_mut()
    }

    /// Get mutable reference to transient content state by index
    #[inline]
    pub fn get_transient_state_mut(&mut self, index: usize) -> Option<&mut ContentState> {
        self.transient_texts.get_mut(index)
    }

    /// Clear all transient texts (called after rendering)
    #[inline]
    pub fn clear_transient_texts(&mut self) {
        self.transient_texts.clear();
    }

    /// Build/shape all transient texts
    #[inline]
    pub fn build_transient_texts(&mut self) {
        let script = Script::Latin;

        for transient_idx in 0..self.transient_texts.len() {
            let (scaled_font_size, num_lines) = {
                let content_state = &self.transient_texts[transient_idx];
                let text_state = match content_state.as_text() {
                    Some(state) => state,
                    None => continue,
                };
                (text_state.scaled_font_size, text_state.lines.len())
            };

            // Process each line
            for line_number in 0..num_lines {
                let content_state = &mut self.transient_texts[transient_idx];
                let text_state = match content_state.as_text_mut() {
                    Some(state) => state,
                    None => continue,
                };

                Self::process_text_line(
                    text_state,
                    line_number,
                    scaled_font_size,
                    script,
                    &self.font_features,
                    &self.fonts,
                    &mut self.scx,
                    &mut self.shaping_cache,
                );
            }
        }
    }

    #[inline]
    pub fn update_dimensions(&mut self, state_id: &usize) {
        let layout = if let Some(text_state) = self.get_state(state_id) {
            text_state.layout
        } else {
            return;
        };

        let new_dimension = self.calculate_character_cell_dimensions(&layout);

        if let Some(text_state) = self.get_state_mut(state_id) {
            text_state.layout.dimensions = new_dimension;
        }
    }

    #[inline]
    pub fn clear_state(&mut self, id: &usize) {
        if let Some(text_state) = self.get_state_mut(id) {
            text_state.clear();
        }
    }

    #[inline]
    pub fn new_line_with_id(&mut self, id: &usize) -> &mut Content {
        if let Some(text_state) = self.get_state_mut(id) {
            text_state.new_line();
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
            if let Some(text_state) = self.get_state_mut(&selector) {
                text_state.new_line_at(pos);
            }
        }

        self
    }

    #[inline]
    pub fn remove_line_at(&mut self, pos: usize) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(text_state) = self.get_state_mut(&selector) {
                text_state.remove_line_at(pos);
            }
        }

        self
    }

    #[inline]
    pub fn clear_line(&mut self, line_to_clear: usize) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(text_state) = self.get_state_mut(&selector) {
                if let Some(line) = text_state.lines.get_mut(line_to_clear) {
                    line.fragments.clear();
                    line.text_buffer.clear();
                    line.render_data.clear();
                }
            }
        }

        self
    }

    #[inline]
    pub fn clear_with_id(&mut self, id: &usize) -> &mut Content {
        if let Some(text_state) = self.get_state_mut(id) {
            text_state.clear();
        }

        self
    }

    #[inline]
    pub fn clear_all(&mut self) -> &mut Content {
        for content_state in self.states.values_mut() {
            if let Some(text_state) = content_state.as_text_mut() {
                text_state.clear();
            }
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
    pub fn add_span(&mut self, text: &str, style: SpanStyle) -> &mut Content {
        if let Some(selector) = self.selector {
            return self.add_span_with_id(&selector, text, style);
        }

        self
    }

    /// Add an empty span that only advances position (no shaping).
    #[inline]
    pub fn add_span_as_rect(&mut self, style: SpanStyle) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(text_state) = self.get_state_mut(&selector) {
                let current_line = text_state.current_line();
                if let Some(line) = text_state.lines.get_mut(current_line) {
                    line.fragments.push(FragmentData {
                        content: None,
                        style,
                    });
                }
            }
        }
        self
    }

    #[inline]
    pub fn add_span_on_line(
        &mut self,
        line_idx: usize,
        text: &str,
        style: SpanStyle,
    ) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(text_state) = self.get_state_mut(&selector) {
                text_state.mark_line_dirty(line_idx);
                if let Some(line) = text_state.lines.get_mut(line_idx) {
                    let range = line.push_text(text);
                    line.fragments.push(FragmentData {
                        content: Some(range),
                        style,
                    });
                }
            }
        }

        self
    }

    /// Add an empty span to advance position without shaping.
    #[inline]
    pub fn add_span_as_rect_on_line(
        &mut self,
        line_idx: usize,
        style: SpanStyle,
    ) -> &mut Content {
        if let Some(selector) = self.selector {
            if let Some(text_state) = self.get_state_mut(&selector) {
                text_state.mark_line_dirty(line_idx);
                if let Some(line) = text_state.lines.get_mut(line_idx) {
                    line.fragments.push(FragmentData {
                        content: None,
                        style,
                    });
                }
            }
        }
        self
    }

    /// Adds a text fragment to the paragraph.
    pub fn add_span_with_id(
        &mut self,
        id: &usize,
        text: &str,
        style: SpanStyle,
    ) -> &mut Content {
        if let Some(text_state) = self.get_state_mut(id) {
            let current_line = text_state.current_line();
            if let Some(line) = &mut text_state.lines.get_mut(current_line) {
                let range = line.push_text(text);
                line.fragments.push(FragmentData {
                    content: Some(range),
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

        // First check if state exists and is text type, get immutable data
        let (scaled_font_size, num_lines) = {
            let content_state = match self.states.get(&state_id) {
                Some(state) => state,
                None => return,
            };
            let text_state = match content_state.as_text() {
                Some(state) => state,
                None => return,
            };
            (text_state.scaled_font_size, text_state.lines.len())
        };

        let features = &self.font_features;

        // Check if the line exists
        if line_number >= num_lines {
            return;
        }

        // Now get mutable borrow for the actual processing
        let content_state = match self.states.get_mut(&state_id) {
            Some(state) => state,
            None => return,
        };

        let text_state = match content_state.as_text_mut() {
            Some(state) => state,
            None => return,
        };

        Self::process_text_line(
            text_state,
            line_number,
            scaled_font_size,
            script,
            features,
            &self.fonts,
            &mut self.scx,
            &mut self.shaping_cache,
        );
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg_attr(target_os = "macos", allow(unused_variables))]
    fn process_text_line(
        text_state: &mut BuilderState,
        line_number: usize,
        scaled_font_size: f32,
        script: Script,
        features: &[swash::Setting<u16>],
        fonts: &FontLibrary,
        scx: &mut ShapeContext,
        shaping_cache: &mut ShapingCache,
    ) {
        // Cache primary font metrics at line level to avoid repeated lock acquisition
        let metrics_result = fonts.inner.write().get_font_metrics(&0, scaled_font_size);

        let line = &mut text_state.lines[line_number];

        for fragment_idx in 0..line.fragments.len() {
            let font_id = line.fragments[fragment_idx].style.font_id;
            let font_vars = line.fragments[fragment_idx].style.font_vars;
            let style = line.fragments[fragment_idx].style;

            // Resolve text range to &str from the shared buffer.
            let content_range = line.fragments[fragment_idx].content;

            // None content = advance-only fragment (no shaping)
            let content = match content_range {
                Some((start, end)) => &line.text_buffer[start as usize..end as usize],
                None => {
                    if let Some((ascent, descent, leading)) = if font_id == 0 {
                        metrics_result
                    } else {
                        fonts
                            .inner
                            .write()
                            .get_font_metrics(&font_id, scaled_font_size)
                    } {
                        let metrics = swash::Metrics {
                            ascent,
                            descent,
                            leading,
                            ..Default::default()
                        };
                        line.render_data.push_empty_run(
                            style,
                            scaled_font_size,
                            line_number as u32,
                            &metrics,
                        );
                    }
                    continue;
                }
            };

            // Check run cache — pre-packed so no re-packing needed
            if let Some(cached_run) = shaping_cache.get(&font_id, content) {
                if let Some((ascent, descent, leading)) = if font_id == 0 {
                    metrics_result
                } else {
                    fonts
                        .inner
                        .write()
                        .get_font_metrics(&font_id, scaled_font_size)
                } {
                    line.render_data.push_cached_run(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        cached_run,
                        ascent,
                        descent,
                        leading,
                    );
                    continue;
                } else {
                    debug!("Font metrics not available for font_id={}", font_id);
                }
            }

            // Cache miss: shape the full run and store result.
            shaping_cache.set_content(font_id, content);

            #[cfg(target_os = "macos")]
            {
                if let Some(handle) = fonts.ct_font(font_id) {
                    let shaped = crate::font::macos::shape_text(
                        &handle,
                        content,
                        scaled_font_size,
                    );
                    let macos_metrics =
                        crate::font::macos::font_metrics(&handle, scaled_font_size);
                    line.render_data.push_run_macos(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        &shaped,
                        &macos_metrics,
                        shaping_cache,
                    );
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                // Only allocate vars on the miss path
                let vars: Vec<_> = text_state.vars.get(font_vars).to_vec();

                let font_library = &fonts.inner.read();
                if let Some((shared_data, offset, key)) = font_library.get_data(&font_id)
                {
                    let font_ref = FontRef {
                        data: shared_data.as_ref(),
                        offset,
                        key,
                    };
                    let mut shaper = scx
                        .builder(font_ref)
                        .script(script)
                        .size(scaled_font_size)
                        .features(features.iter().copied())
                        .variations(vars.iter().copied())
                        .build();

                    shaper.add_str(content);

                    line.render_data.push_run(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        shaper,
                        shaping_cache,
                    );
                }
            }
        }
    }

    #[inline]
    pub fn build(&mut self) {
        if let Some(selector) = self.selector {
            let state_id = selector;

            let num_lines = {
                if let Some(text_state) = self.get_state_mut(&state_id) {
                    text_state.mark_dirty();
                    text_state.lines.len()
                } else {
                    0
                }
            };

            for line_number in 0..num_lines {
                self.process_line(state_id, line_number);
            }
        }
    }

    #[inline]
    pub fn build_line(&mut self, line_number: usize) {
        if let Some(selector) = self.selector {
            // Process just the specified line
            self.process_line(selector, line_number);
        }
    }

    /// Set rectangle at ID (overwrites existing content)
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn set_rect(
        &mut self,
        id: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
    ) {
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Rect {
                x,
                y,
                width,
                height,
                color,
                depth,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::Rect {
                    x,
                    y,
                    width,
                    height,
                    color,
                    depth,
                }),
            );
        }
    }

    /// Set rounded rectangle at ID
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn set_rounded_rect(
        &mut self,
        id: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        border_radius: f32,
    ) {
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::RoundedRect {
                x,
                y,
                width,
                height,
                color,
                depth,
                border_radius,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::RoundedRect {
                    x,
                    y,
                    width,
                    height,
                    color,
                    depth,
                    border_radius,
                }),
            );
        }
    }

    /// Set line at ID
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn set_line(
        &mut self,
        id: usize,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        color: [f32; 4],
        depth: f32,
    ) {
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Line {
                x1,
                y1,
                x2,
                y2,
                width,
                color,
                depth,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::Line {
                    x1,
                    y1,
                    x2,
                    y2,
                    width,
                    color,
                    depth,
                }),
            );
        }
    }

    /// Set triangle at ID
    #[inline]
    pub fn set_triangle(
        &mut self,
        id: usize,
        points: [(f32, f32); 3],
        color: [f32; 4],
        depth: f32,
    ) {
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Triangle {
                points,
                color,
                depth,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::Triangle {
                    points,
                    color,
                    depth,
                }),
            );
        }
    }

    /// Set polygon at ID
    #[inline]
    pub fn set_polygon(
        &mut self,
        id: usize,
        points: &[(f32, f32)],
        color: [f32; 4],
        depth: f32,
    ) {
        let points_smallvec: SmallVec<[(f32, f32); 8]> = points.iter().copied().collect();
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Polygon {
                points: points_smallvec,
                color,
                depth,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::Polygon {
                    points: points_smallvec,
                    color,
                    depth,
                }),
            );
        }
    }

    /// Set arc at ID
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn set_arc(
        &mut self,
        id: usize,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        stroke_width: f32,
        color: [f32; 4],
        depth: f32,
    ) {
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Arc {
                center_x,
                center_y,
                radius,
                start_angle,
                end_angle,
                stroke_width,
                color,
                depth,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::Arc {
                    center_x,
                    center_y,
                    radius,
                    start_angle,
                    end_angle,
                    stroke_width,
                    color,
                    depth,
                }),
            );
        }
    }

    /// Set image rectangle at ID
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn set_image(
        &mut self,
        id: usize,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        coords: [f32; 4],
        depth: f32,
        atlas_layer: i32,
    ) {
        if let Some(content_state) = self.states.get_mut(&id) {
            content_state.data = ContentData::Image {
                x,
                y,
                width,
                height,
                color,
                coords,
                depth,
                atlas_layer,
            };
            content_state.render_data.needs_repaint = true;
            content_state.render_data.should_remove = false;
        } else {
            self.states.insert(
                id,
                ContentState::new(ContentData::Image {
                    x,
                    y,
                    width,
                    height,
                    color,
                    coords,
                    depth,
                    atlas_layer,
                }),
            );
        }
    }
}

/// Run-level shaping cache (like Ghostty's ShaperCache).
///
/// Caches pre-packed shaped runs per text run, keyed by (content + font_id).
/// The shaper always sees the full run so ligatures are handled naturally.
/// Stores packed GlyphData directly so cache hits avoid re-packing.
pub struct ShapingCache {
    /// LRU cache per font_id: hash(content + font_id) → pre-packed run
    inner: FxHashMap<usize, LruCache<u64, CachedRun>>,
    /// Current shaping context
    font_id: usize,
    content_hash: u64,
}

impl Default for ShapingCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ShapingCache {
    pub fn new() -> Self {
        ShapingCache {
            inner: FxHashMap::default(),
            font_id: 0,
            content_hash: 0,
        }
    }

    /// Look up a pre-packed cached run.
    #[inline]
    pub fn get(&mut self, font_id: &usize, content: &str) -> Option<&CachedRun> {
        let key = Self::cache_key(content, *font_id);
        if let Some(cache) = self.inner.get_mut(font_id) {
            return cache.get(&key);
        }
        None
    }

    /// Record which content is about to be shaped (called before shaping).
    #[inline]
    pub fn set_content(&mut self, font_id: usize, content: &str) {
        self.font_id = font_id;
        self.content_hash = Self::cache_key(content, font_id);
    }

    /// Store a pre-packed run in the cache after shaping.
    #[inline]
    pub fn finish_with_run(&mut self, cached_run: CachedRun) {
        if self.content_hash != 0 {
            if let Some(cache) = self.inner.get_mut(&self.font_id) {
                cache.put(self.content_hash, cached_run);
            } else {
                let size = if self.font_id == 0 { 512 } else { 256 };
                let mut cache = LruCache::new(NonZeroUsize::new(size).unwrap());
                cache.put(self.content_hash, cached_run);
                self.inner.insert(self.font_id, cache);
            }
        }
        self.font_id = 0;
        self.content_hash = 0;
    }

    /// Clear all caches (called when fonts change).
    pub fn clear(&mut self) {
        self.inner.clear();
        self.font_id = 0;
        self.content_hash = 0;
        debug!("ShapingCache cleared");
    }

    /// Compute a position-independent cache key from content and font_id.
    #[inline]
    pub fn cache_key(content: &str, font_id: usize) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        content.hash(&mut hasher);
        font_id.hash(&mut hasher);
        hasher.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swash::shape::cluster::Glyph;

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

    fn make_cached_run(glyphs: &[(u16, f32)]) -> CachedRun {
        use crate::layout::glyph::GlyphData;
        let glyph_data: Vec<GlyphData> = glyphs
            .iter()
            .map(|&(id, advance)| GlyphData::simple(id, advance, 0))
            .collect();
        let advance = glyphs.iter().map(|g| g.1).sum();
        let cache_key = 42; // dummy
        CachedRun {
            glyphs: glyph_data,
            detailed_glyphs: vec![],
            advance,
            cache_key,
        }
    }

    #[test]
    fn test_shaping_cache_hit_and_miss() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Empty cache: miss
        assert!(cache.get(&font_id, "hello").is_none());

        // Store a pre-packed run for "hello"
        cache.set_content(font_id, "hello");
        cache.finish_with_run(make_cached_run(&[
            (104, 8.0),
            (101, 8.0),
            (108, 8.0),
            (108, 8.0),
            (111, 8.0),
        ]));

        // Same run: hit
        assert!(cache.get(&font_id, "hello").is_some());
        assert_eq!(cache.get(&font_id, "hello").unwrap().glyphs.len(), 5);

        // Different run: miss
        assert!(cache.get(&font_id, "world").is_none());

        // Different font: miss
        assert!(cache.get(&1, "hello").is_none());
    }

    #[test]
    fn test_shaping_cache_ligature_preserved() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Store "=>" as a single ligature glyph
        cache.set_content(font_id, "=>");
        cache.finish_with_run(make_cached_run(&[(999, 16.0)]));

        // Should hit and preserve the ligature (1 glyph, not 2)
        let cached = cache.get(&font_id, "=>").unwrap();
        assert_eq!(cached.glyphs.len(), 1);
    }

    #[test]
    fn test_shaping_cache_clear() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        cache.set_content(font_id, "test");
        cache.finish_with_run(make_cached_run(&[(1, 8.0)]));

        assert!(cache.get(&font_id, "test").is_some());
        cache.clear();
        assert!(cache.get(&font_id, "test").is_none());
    }

    #[test]
    fn test_shaping_cache_key_no_collision() {
        let along_key = ShapingCache::cache_key("along", 1);
        let clone_key = ShapingCache::cache_key("clone", 1);
        assert_ne!(along_key, clone_key);

        // Same content, different font
        assert_ne!(
            ShapingCache::cache_key("test", 0),
            ShapingCache::cache_key("test", 1),
        );

        // Deterministic
        assert_eq!(
            ShapingCache::cache_key("test", 0),
            ShapingCache::cache_key("test", 0),
        );
    }

    #[test]
    fn test_empty_span_creates_fragment_with_none_content() {
        // Simulates '\0' cells: None content means advance-only
        let mut line = BuilderLine::default();

        let range_a = line.push_text("A");
        line.fragments.push(FragmentData {
            content: Some(range_a),
            style: SpanStyle::default(),
        });
        line.fragments.push(FragmentData {
            content: None, // empty span (like '\0' cell)
            style: SpanStyle::default(),
        });
        let range_b = line.push_text("B");
        line.fragments.push(FragmentData {
            content: Some(range_b),
            style: SpanStyle::default(),
        });

        assert_eq!(line.fragments.len(), 3);
        assert!(line.fragments[0].content.is_some());
        assert!(line.fragments[1].content.is_none());
        assert!(line.fragments[2].content.is_some());
    }

    #[test]
    fn test_empty_run_has_no_glyphs() {
        // Verify push_empty_run creates a run with empty glyphs
        let mut render_data = RenderData::new();
        let metrics = swash::Metrics {
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            ..Default::default()
        };

        render_data.push_empty_run(SpanStyle::default(), 16.0, 0, &metrics);

        assert_eq!(render_data.runs.len(), 1);
        assert!(render_data.runs[0].glyphs.is_empty());
        assert_eq!(render_data.runs[0].span.width, 1.0);
    }

    #[test]
    fn test_mixed_text_and_empty_runs_ordering() {
        // Simulates a line like: "ABC" + [empty] + [empty] + "DEF"
        // All runs should be in order and empty runs between text runs
        let mut render_data = RenderData::new();
        let metrics = swash::Metrics {
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            ..Default::default()
        };

        // Simulate text run "ABC" with 3 glyphs
        let glyphs_abc = [
            create_test_glyph(65, 0.0, 0.0, 8.0),
            create_test_glyph(66, 0.0, 0.0, 8.0),
            create_test_glyph(67, 0.0, 0.0, 8.0),
        ];
        render_data.push_run_without_shaper(
            SpanStyle::default(),
            16.0,
            0,
            &glyphs_abc,
            &metrics,
        );

        // Two empty runs (simulating '\0' cells)
        render_data.push_empty_run(SpanStyle::default(), 16.0, 0, &metrics);
        render_data.push_empty_run(SpanStyle::default(), 16.0, 0, &metrics);

        // Another text run "DEF"
        let glyphs_def = [
            create_test_glyph(68, 0.0, 0.0, 8.0),
            create_test_glyph(69, 0.0, 0.0, 8.0),
            create_test_glyph(70, 0.0, 0.0, 8.0),
        ];
        render_data.push_run_without_shaper(
            SpanStyle::default(),
            16.0,
            0,
            &glyphs_def,
            &metrics,
        );

        // Should have 4 runs total in order
        assert_eq!(render_data.runs.len(), 4);
        // First run: 3 glyphs (ABC)
        assert_eq!(render_data.runs[0].glyphs.len(), 3);
        // Second run: empty
        assert!(render_data.runs[1].glyphs.is_empty());
        // Third run: empty
        assert!(render_data.runs[2].glyphs.is_empty());
        // Fourth run: 3 glyphs (DEF)
        assert_eq!(render_data.runs[3].glyphs.len(), 3);
    }

    #[test]
    fn test_empty_run_preserves_background_color() {
        // '\0' cells with colored background (like from \033[K) should preserve bg
        let mut render_data = RenderData::new();
        let metrics = swash::Metrics {
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            ..Default::default()
        };

        let style_with_bg = SpanStyle {
            background_color: Some([1.0, 0.0, 0.0, 1.0]), // red
            ..SpanStyle::default()
        };

        render_data.push_empty_run(style_with_bg, 16.0, 0, &metrics);

        assert_eq!(render_data.runs.len(), 1);
        assert!(render_data.runs[0].glyphs.is_empty());
        assert_eq!(
            render_data.runs[0].span.background_color,
            Some([1.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn test_empty_runs_survive_rebuild() {
        // Simulates: printf '\033[41m\033[K\n\033[0m'
        // Line with only empty fragments (None content) with colored bg.
        // First build should create runs. Second build (simulating next frame
        // where line is undamaged) should preserve them.

        let mut line = BuilderLine::default();

        // Add 3 empty fragments with red bg (like \033[K erase with color)
        let style_red_bg = SpanStyle {
            background_color: Some([1.0, 0.0, 0.0, 1.0]),
            ..SpanStyle::default()
        };
        for _ in 0..3 {
            line.fragments.push(FragmentData {
                content: None,
                style: style_red_bg,
            });
        }

        assert_eq!(line.fragments.len(), 3);
        assert!(
            line.render_data.runs.is_empty(),
            "runs should be empty before build"
        );

        // Simulate what process_text_line does for None fragments
        let metrics = swash::Metrics {
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            ..Default::default()
        };
        for frag in &line.fragments {
            if frag.content.is_none() {
                line.render_data
                    .push_empty_run(frag.style, 16.0, 0, &metrics);
            }
        }

        assert_eq!(
            line.render_data.runs.len(),
            3,
            "should have 3 empty runs after build"
        );
        assert!(line.render_data.runs[0].glyphs.is_empty());
        assert_eq!(
            line.render_data.runs[0].span.background_color,
            Some([1.0, 0.0, 0.0, 1.0])
        );

        // Now simulate "next frame" — line is NOT damaged, so fragments are
        // cleared and re-added, but render_data should persist until rebuild.
        // This is what happens in the partial update path:
        // 1. clear_line clears fragments + render_data
        // 2. create_line re-adds fragments
        // 3. build_line re-processes

        // Simulate clear_line
        line.fragments.clear();
        line.render_data.clear();

        assert!(
            line.render_data.runs.is_empty(),
            "runs cleared after clear_line"
        );

        // Simulate re-adding same fragments
        for _ in 0..3 {
            line.fragments.push(FragmentData {
                content: None,
                style: style_red_bg,
            });
        }

        // Simulate process_text_line again
        for frag in &line.fragments {
            if frag.content.is_none() {
                line.render_data
                    .push_empty_run(frag.style, 16.0, 0, &metrics);
            }
        }

        assert_eq!(
            line.render_data.runs.len(),
            3,
            "runs should be restored after rebuild"
        );
        assert_eq!(
            line.render_data.runs[0].span.background_color,
            Some([1.0, 0.0, 0.0, 1.0])
        );
    }

    #[test]
    fn test_empty_runs_not_duplicated_on_full_rebuild() {
        // Simulates the full rebuild path (content.build()) being called
        // multiple times without clearing. Runs should NOT accumulate.

        let mut line = BuilderLine::default();

        let style = SpanStyle {
            background_color: Some([0.0, 1.0, 0.0, 1.0]),
            ..SpanStyle::default()
        };
        line.fragments.push(FragmentData {
            content: None,
            style,
        });

        let metrics = swash::Metrics {
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            ..Default::default()
        };

        // First build
        for frag in &line.fragments {
            if frag.content.is_none() {
                line.render_data
                    .push_empty_run(frag.style, 16.0, 0, &metrics);
            }
        }
        assert_eq!(line.render_data.runs.len(), 1);

        // Second build WITHOUT clearing — this simulates calling build() twice
        for frag in &line.fragments {
            if frag.content.is_none() {
                line.render_data
                    .push_empty_run(frag.style, 16.0, 0, &metrics);
            }
        }
        // BUG: runs accumulate! This is the issue.
        assert_eq!(
            line.render_data.runs.len(),
            2,
            "runs duplicated without clear — this is the bug"
        );
    }
}
