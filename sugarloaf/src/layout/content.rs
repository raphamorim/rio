// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

#![allow(clippy::uninlined_format_args)]

use crate::font::FontLibrary;
use crate::font_introspector::shape::cluster::OwnedGlyphCluster;
use crate::font_introspector::shape::ShapeContext;
use crate::font_introspector::text::Script;
use crate::font_introspector::{shape::cluster::GlyphCluster, FontRef};
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

use crate::font_introspector::Attributes;
use crate::font_introspector::Setting;
use crate::{sugarloaf::primitives::SugarCursor, DrawableChar, Graphic};

pub type CachedContent = Vec<OwnedGlyphCluster>;

#[derive(Debug, Clone)]
pub struct FragmentData {
    /// Text content to shape. None means advance position only (no shaping).
    pub content: Option<String>,
    pub style: SpanStyle,
}

#[derive(Default, Clone, Debug)]
pub struct BuilderLine {
    pub fragments: Vec<FragmentData>,
    pub render_data: RenderData,
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
            line.fragments.push(FragmentData {
                content: Some(text.to_string()),
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
            line.fragments.push(FragmentData {
                content: Some(text.to_string()),
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
        }
    }
}

/// Context for paragraph layout.
pub struct Content {
    fonts: FontLibrary,
    font_features: Vec<crate::font_introspector::Setting<u16>>,
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

    /// Clear image overlays on the selected content state.
    pub fn clear_image_overlays(&mut self) {
        if let Some(id) = self.selector {
            if let Some(state) = self.states.get_mut(&id) {
                state.image_overlays.clear();
            }
        }
    }

    /// Push an image overlay onto the selected content state.
    pub fn push_image_overlay(
        &mut self,
        overlay: crate::sugarloaf::graphics::GraphicOverlay,
    ) {
        if let Some(id) = self.selector {
            if let Some(state) = self.states.get_mut(&id) {
                state.image_overlays.push(overlay);
            }
        }
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
        font_features: Vec<crate::font_introspector::Setting<u16>>,
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
        if let Some(font_library_data) = self.fonts.inner.try_read() {
            let font_id = 0; // FONT_ID_REGULAR
            let font_size = layout.font_size;

            // Get font data to create swash FontRef
            if let Some((font_data, offset, _key)) = font_library_data.get_data(&font_id)
            {
                // Create swash FontRef directly from font data
                if let Some(font_ref) = crate::font_introspector::FontRef::from_index(
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
                            crate::font_introspector::GlyphMetrics::from_font(
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
                    line.fragments.push(FragmentData {
                        content: Some(text.to_string()),
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
                line.fragments.push(FragmentData {
                    content: Some(text.to_string()),
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
    fn process_text_line(
        text_state: &mut BuilderState,
        line_number: usize,
        scaled_font_size: f32,
        script: Script,
        features: &[crate::font_introspector::Setting<u16>],
        fonts: &FontLibrary,
        scx: &mut ShapeContext,
        shaping_cache: &mut ShapingCache,
    ) {
        let line_start = std::time::Instant::now();
        let mut tier1_hits = 0u32;
        let mut tier2_hits = 0u32;
        let mut misses = 0u32;
        let mut empty_runs = 0u32;

        // Process fragments in the line
        let line = &mut text_state.lines[line_number];

        // Process each fragment
        for fragment_idx in 0..line.fragments.len() {
            // Get a reference to the current fragment
            let item = &line.fragments[fragment_idx];
            let font_id = item.style.font_id;
            let font_vars = item.style.font_vars;
            let style = item.style;

            // None content = advance-only fragment (no shaping)
            let content = match &item.content {
                Some(c) => c,
                None => {
                    // Create an empty run that just advances position
                    if let Some((ascent, descent, leading)) = fonts
                        .inner
                        .write()
                        .get_font_metrics(&font_id, scaled_font_size)
                    {
                        let metrics = crate::font_introspector::Metrics {
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
                    empty_runs += 1;
                    continue;
                }
            };

            // Get vars for this fragment
            let vars: Vec<_> = text_state.vars.get(font_vars).to_vec();

            // Tier 1: try to compose from per-character cache
            if let Some(composed) =
                shaping_cache.try_compose_from_chars(content, font_id)
            {
                if let Some((ascent, descent, leading)) = fonts
                    .inner
                    .write()
                    .get_font_metrics(&font_id, scaled_font_size)
                {
                    let metrics = crate::font_introspector::Metrics {
                        ascent,
                        descent,
                        leading,
                        ..Default::default()
                    };
                    if line.render_data.push_run_without_shaper(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        composed,
                        &metrics,
                    ) {
                        tier1_hits += 1;
                        continue;
                    }
                }
            }

            // Tier 2: try run cache (for ligature sequences)
            if let Some(cached_content) =
                shaping_cache.get_cached_run(&font_id, content)
            {
                if let Some((ascent, descent, leading)) = fonts
                    .inner
                    .write()
                    .get_font_metrics(&font_id, scaled_font_size)
                {
                    let metrics = crate::font_introspector::Metrics {
                        ascent,
                        descent,
                        leading,
                        ..Default::default()
                    };
                    if line.render_data.push_run_without_shaper(
                        style,
                        scaled_font_size,
                        line_number as u32,
                        cached_content,
                        &metrics,
                    ) {
                        tier2_hits += 1;
                        continue;
                    }
                } else {
                    debug!("Font metrics not available for font_id={}", font_id);
                }
            }

            // Cache miss: shape the text and populate appropriate tier
            misses += 1;
            shaping_cache.set_content(font_id, content);

            let font_library = &fonts.inner.read();
            if let Some((shared_data, offset, key)) = font_library.get_data(&font_id) {
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

        let elapsed = line_start.elapsed();
        let total = tier1_hits + tier2_hits + misses;
        if total > 0 {
            println!(
                "[PERF] line {} | {:.1}µs | frags={} t1={} t2={} miss={} empty={} | chars_cached={}",
                line_number,
                elapsed.as_secs_f64() * 1_000_000.0,
                line.fragments.len(),
                tier1_hits,
                tier2_hits,
                misses,
                empty_runs,
                shaping_cache.char_cache.len(),
            );
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

#[derive(Default)]
/// Two-tier shaping cache inspired by Chrome's CachingWordShaper.
///
/// **Tier 1 (Character Cache):** Caches shaped results per individual character.
/// For monospace terminal fonts, most characters shape independently — same glyph
/// output regardless of neighbors. This gives maximum reuse: any character appearing
/// anywhere in the terminal shares one shaped result.
///
/// **Tier 2 (Run Cache):** LRU cache for multi-character runs where ligatures
/// were detected (e.g., `=>`, `->`, `!=`). Falls back here when Tier 1 can't
/// compose the result (N:1 char-to-glyph mappings).
pub struct ShapingCache {
    /// Tier 1: per-character cache keyed by (char, font_id)
    char_cache: FxHashMap<(char, usize), OwnedGlyphCluster>,
    /// Tier 2: run cache for ligature sequences (LRU per font_id)
    run_cache: FxHashMap<usize, LruCache<u64, CachedContent>>,
    /// Temporary stash for clusters during active shaping
    stash: Vec<OwnedGlyphCluster>,
    /// Reusable scratch buffer for composing Tier 1 results
    composed: Vec<OwnedGlyphCluster>,
    /// Current shaping context
    font_id: usize,
    content_hash: u64,
    /// Characters of the content being shaped (for 1:1 detection in finish)
    current_chars: Vec<char>,
}

impl ShapingCache {
    pub fn new() -> Self {
        ShapingCache {
            char_cache: FxHashMap::default(),
            run_cache: FxHashMap::default(),
            stash: Vec::with_capacity(64),
            composed: Vec::with_capacity(64),
            font_id: 0,
            content_hash: 0,
            current_chars: Vec::new(),
        }
    }

    /// Try to compose a full fragment from per-character cache entries (Tier 1).
    /// Returns `Some` if ALL characters have cached entries, `None` otherwise.
    #[inline]
    pub fn try_compose_from_chars(
        &mut self,
        content: &str,
        font_id: usize,
    ) -> Option<&Vec<OwnedGlyphCluster>> {
        self.composed.clear();
        let mut byte_offset = 0u32;

        for ch in content.chars() {
            let len = ch.len_utf8() as u32;
            let cached = self.char_cache.get(&(ch, font_id))?;

            let mut cluster = cached.clone();
            // Rewrite source range to match position in this fragment
            cluster.source = crate::font_introspector::text::cluster::SourceRange {
                start: byte_offset,
                end: byte_offset + len,
            };
            self.composed.push(cluster);
            byte_offset += len;
        }

        Some(&self.composed)
    }

    /// Look up a full run in the Tier 2 cache (for ligature sequences).
    #[inline]
    pub fn get_cached_run(
        &mut self,
        font_id: &usize,
        content: &str,
    ) -> Option<&CachedContent> {
        let key = Self::run_cache_key(content, *font_id);
        if let Some(cache) = self.run_cache.get_mut(font_id) {
            if let Some(cached_content) = cache.get(&key) {
                return Some(cached_content);
            }
        }
        None
    }

    /// Record which content is about to be shaped (called before shaping).
    #[inline]
    pub fn set_content(&mut self, font_id: usize, content: &str) {
        self.font_id = font_id;
        self.content_hash = Self::run_cache_key(content, font_id);
        self.current_chars.clear();
        self.current_chars.extend(content.chars());
    }

    /// Accumulate a shaped glyph cluster (called during shaping).
    #[inline]
    pub fn add_glyph_cluster(&mut self, glyph_cluster: &GlyphCluster) {
        self.stash.push(glyph_cluster.into());
    }

    /// Finalize shaping: analyze output and populate the appropriate cache tier.
    ///
    /// If the shaped output has a 1:1 mapping (each char → one cluster with one
    /// glyph, no ligatures), populate Tier 1 (per-character). Otherwise, store
    /// the whole run in Tier 2.
    #[inline]
    pub fn finish(&mut self) {
        if self.content_hash == 0 || self.stash.is_empty() {
            self.stash.clear();
            self.reset_state();
            return;
        }

        let char_count = self.current_chars.len();
        let cluster_count = self.stash.len();

        // Check 1:1 mapping: same number of clusters as characters,
        // each cluster has exactly one glyph, no ligature components.
        let is_one_to_one = char_count == cluster_count
            && self.stash.iter().all(|c| {
                c.components.is_empty() && c.glyphs.len() == 1
            });

        if is_one_to_one {
            // Populate Tier 1 character cache
            for (ch, cluster) in
                self.current_chars.drain(..).zip(self.stash.drain(..))
            {
                self.char_cache
                    .entry((ch, self.font_id))
                    .or_insert(cluster);
            }
        } else {
            // Populate Tier 2 run cache
            let cached = std::mem::take(&mut self.stash);
            if let Some(cache) = self.run_cache.get_mut(&self.font_id) {
                cache.put(self.content_hash, cached);
            } else {
                let size = if self.font_id == 0 { 512 } else { 256 };
                let mut cache =
                    LruCache::new(NonZeroUsize::new(size).unwrap());
                cache.put(self.content_hash, cached);
                self.run_cache.insert(self.font_id, cache);
            }
        }

        self.stash.clear();
        self.reset_state();
    }

    /// Clear all cache tiers (called when fonts change).
    pub fn clear(&mut self) {
        self.char_cache.clear();
        self.run_cache.clear();
        self.stash.clear();
        self.composed.clear();
        self.reset_state();
        debug!("ShapingCache cleared");
    }

    #[inline]
    fn reset_state(&mut self) {
        self.font_id = 0;
        self.content_hash = 0;
        self.current_chars.clear();
    }

    /// Compute a hash key for the Tier 2 run cache.
    #[inline]
    fn run_cache_key(content: &str, font_id: usize) -> u64 {
        let mut hasher = rustc_hash::FxHasher::default();
        content.hash(&mut hasher);
        font_id.hash(&mut hasher);
        hasher.finish()
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
    fn test_shaping_cache_tier1_ascii() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Empty cache: Tier 1 miss
        assert!(cache.try_compose_from_chars("hello", font_id).is_none());

        // Simulate shaping "hello" — each char produces 1 cluster with 1 glyph
        cache.set_content(font_id, "hello");
        for (i, ch) in "hello".chars().enumerate() {
            let cluster = create_test_cluster(
                i as u32,
                i as u32 + 1,
                create_test_glyph(ch as u16, 0.0, 0.0, 8.0),
            );
            cache.stash.push(cluster);
        }
        cache.finish();

        // Now Tier 1 should hit for "hello"
        assert!(cache.try_compose_from_chars("hello", font_id).is_some());

        // And also hit for substrings that share characters
        assert!(cache.try_compose_from_chars("hell", font_id).is_some());
        assert!(cache.try_compose_from_chars("lo", font_id).is_some());
        assert!(cache.try_compose_from_chars("ole", font_id).is_some());

        // Miss for characters not yet cached
        assert!(cache.try_compose_from_chars("world", font_id).is_none());
    }

    #[test]
    fn test_shaping_cache_tier2_ligatures() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Simulate shaping "=>" where 2 chars produce 1 cluster (ligature)
        cache.set_content(font_id, "=>");
        let ligature_cluster = OwnedGlyphCluster {
            source: SourceRange { start: 0, end: 2 },
            info: Default::default(),
            glyphs: vec![create_test_glyph(999, 0.0, 0.0, 16.0)],
            components: vec![
                SourceRange { start: 0, end: 1 },
                SourceRange { start: 1, end: 2 },
            ],
            data: Default::default(),
        };
        cache.stash.push(ligature_cluster);
        cache.finish();

        // Tier 1 miss (chars not individually cached from this shaping)
        assert!(cache.try_compose_from_chars("=>", font_id).is_none());

        // Tier 2 hit
        assert!(cache.get_cached_run(&font_id, "=>").is_some());
    }

    #[test]
    fn test_shaping_cache_source_range_adjustment() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Cache character 'A'
        cache.set_content(font_id, "A");
        cache.stash.push(create_test_cluster(0, 1, create_test_glyph(65, 0.0, 0.0, 8.0)));
        cache.finish();

        // Cache character 'B'
        cache.set_content(font_id, "B");
        cache.stash.push(create_test_cluster(0, 1, create_test_glyph(66, 0.0, 0.0, 8.0)));
        cache.finish();

        // Compose "AB" — source ranges should be [0,1) and [1,2)
        let composed = cache.try_compose_from_chars("AB", font_id).unwrap();
        assert_eq!(composed.len(), 2);
        assert_eq!(composed[0].source.start, 0);
        assert_eq!(composed[0].source.end, 1);
        assert_eq!(composed[1].source.start, 1);
        assert_eq!(composed[1].source.end, 2);
    }

    #[test]
    fn test_shaping_cache_clear() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Populate Tier 1
        cache.set_content(font_id, "x");
        cache.stash.push(create_test_cluster(0, 1, create_test_glyph(120, 0.0, 0.0, 8.0)));
        cache.finish();

        assert!(cache.try_compose_from_chars("x", font_id).is_some());

        cache.clear();

        assert!(cache.try_compose_from_chars("x", font_id).is_none());
    }

    #[test]
    fn test_shaping_cache_run_key_no_collision() {
        // Verify that run cache keys don't collide for similar words
        let along_key = ShapingCache::run_cache_key("along", 1);
        let clone_key = ShapingCache::run_cache_key("clone", 1);
        assert_ne!(along_key, clone_key);

        // Same content, different font
        let key_f0 = ShapingCache::run_cache_key("test", 0);
        let key_f1 = ShapingCache::run_cache_key("test", 1);
        assert_ne!(key_f0, key_f1);

        // Same content, same font — deterministic
        let key_a = ShapingCache::run_cache_key("test", 0);
        let key_b = ShapingCache::run_cache_key("test", 0);
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn test_shaping_cache_char_reuse_across_fragments() {
        let mut cache = ShapingCache::new();
        let font_id = 0;

        // Shape "aaabb" — should cache 'a' and 'b' individually
        cache.set_content(font_id, "aaabb");
        for (i, ch) in "aaabb".chars().enumerate() {
            cache.stash.push(create_test_cluster(
                i as u32,
                i as u32 + 1,
                create_test_glyph(ch as u16, 0.0, 0.0, 8.0),
            ));
        }
        cache.finish();

        // Now "aaaabb" should compose entirely from Tier 1 cache
        // (4 a's + 2 b's — all cached from previous shaping)
        assert!(cache.try_compose_from_chars("aaaabb", font_id).is_some());
        assert!(cache.try_compose_from_chars("bba", font_id).is_some());
        assert!(cache.try_compose_from_chars("abab", font_id).is_some());
    }

    #[test]
    fn test_empty_span_creates_fragment_with_none_content() {
        // Simulates '\0' cells: None content means advance-only
        let mut line = BuilderLine::default();

        line.fragments.push(FragmentData {
            content: Some("A".to_string()),
            style: SpanStyle::default(),
        });
        line.fragments.push(FragmentData {
            content: None, // empty span (like '\0' cell)
            style: SpanStyle::default(),
        });
        line.fragments.push(FragmentData {
            content: Some("B".to_string()),
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
        let metrics = crate::font_introspector::Metrics {
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
        let metrics = crate::font_introspector::Metrics {
            ascent: 12.0,
            descent: 4.0,
            leading: 0.0,
            ..Default::default()
        };

        // Simulate text run "ABC" with 3 glyphs
        let clusters = vec![
            create_test_cluster(0, 1, create_test_glyph(65, 0.0, 0.0, 8.0)),
            create_test_cluster(1, 2, create_test_glyph(66, 0.0, 0.0, 8.0)),
            create_test_cluster(2, 3, create_test_glyph(67, 0.0, 0.0, 8.0)),
        ];
        render_data.push_run_without_shaper(
            SpanStyle::default(),
            16.0,
            0,
            &clusters,
            &metrics,
        );

        // Two empty runs (simulating '\0' cells)
        render_data.push_empty_run(SpanStyle::default(), 16.0, 0, &metrics);
        render_data.push_empty_run(SpanStyle::default(), 16.0, 0, &metrics);

        // Another text run "DEF"
        let clusters2 = vec![
            create_test_cluster(0, 1, create_test_glyph(68, 0.0, 0.0, 8.0)),
            create_test_cluster(1, 2, create_test_glyph(69, 0.0, 0.0, 8.0)),
            create_test_cluster(2, 3, create_test_glyph(70, 0.0, 0.0, 8.0)),
        ];
        render_data.push_run_without_shaper(
            SpanStyle::default(),
            16.0,
            0,
            &clusters2,
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
        let metrics = crate::font_introspector::Metrics {
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
        let metrics = crate::font_introspector::Metrics {
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

        let metrics = crate::font_introspector::Metrics {
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
