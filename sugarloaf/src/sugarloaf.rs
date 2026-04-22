pub mod graphics;
pub mod primitives;
pub mod state;

use crate::components::core::image::Handle;
use crate::components::filters::{Filter, FiltersBrush};
use crate::font::{fonts::SugarloafFont, FontLibrary};
use crate::font_cache::{compute_advance, resolve_with, FontCache, ResolvedGlyph};
use crate::font_introspector::Attributes;
use crate::layout::{RootStyle, TextLayout};
use crate::renderer::Renderer;
use crate::sugarloaf::graphics::{GraphicDataEntry, Graphics};

use crate::context::Context;
use crate::Content;
use crate::TextDimensions;
use core::fmt::{Debug, Formatter};
use primitives::ImageProperties;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use state::SugarState;

pub struct Sugarloaf<'a> {
    pub ctx: Context<'a>,
    renderer: Renderer,
    state: state::SugarState,
    pub background_color: Option<wgpu::Color>,
    pub background_image: Option<ImageProperties>,
    pub graphics: Graphics,
    filters_brush: Option<FiltersBrush>,
    /// Pixel data for standalone image textures, keyed by ImageId.
    pub image_data: rustc_hash::FxHashMap<u32, GraphicDataEntry>,
    /// Persistent state for the CPU rasterizer (glyph cache + frame hash).
    /// Unused on GPU backends.
    cpu_cache: crate::renderer::cpu::CpuCache,
    /// Memo of `(char, attrs) -> ResolvedGlyph`. Owned here (next to
    /// the FontLibrary it caches) so frontends never have to track
    /// their own font cache. Each entry carries both terminal-cell
    /// width (for the grid) and unscaled glyph advance (for
    /// proportional UI via `char_advance`).
    font_cache: FontCache,
}

#[derive(Debug)]
pub struct SugarloafErrors {
    pub fonts_not_found: Vec<SugarloafFont>,
}

pub struct SugarloafWithErrors<'a> {
    pub instance: Sugarloaf<'a>,
    pub errors: SugarloafErrors,
}

impl Debug for SugarloafWithErrors<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.errors)
    }
}

#[derive(Copy, Clone)]
pub struct SugarloafWindowSize {
    pub width: f32,
    pub height: f32,
}

pub struct SugarloafWindow {
    pub handle: raw_window_handle::RawWindowHandle,
    pub display: raw_window_handle::RawDisplayHandle,
    pub size: SugarloafWindowSize,
    pub scale: f32,
}

pub enum SugarloafBackend {
    Wgpu(wgpu::Backends),
    #[cfg(target_os = "macos")]
    Metal,
    /// CPU rendering via tiny-skia + softbuffer.
    Cpu,
}

pub struct SugarloafRenderer {
    pub power_preference: wgpu::PowerPreference,
    pub backend: SugarloafBackend,
    pub font_features: Option<Vec<String>>,
    pub colorspace: Colorspace,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Colorspace {
    Srgb,
    DisplayP3,
    Rec2020,
}

#[allow(clippy::derivable_impls)]
impl Default for Colorspace {
    fn default() -> Colorspace {
        // See `rio-backend::config::window::Colorspace::default` — the
        // config field drives how input colors are interpreted, and the
        // shader/surface always target a wide-gamut output. Default sRGB
        // keeps theme bytes visually consistent with the rest of the OS.
        Colorspace::Srgb
    }
}

impl Default for SugarloafRenderer {
    fn default() -> SugarloafRenderer {
        #[cfg(target_arch = "wasm32")]
        let default_backend =
            SugarloafBackend::Wgpu(wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL);

        #[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
        let default_backend = SugarloafBackend::Wgpu(wgpu::Backends::all());

        #[cfg(all(target_os = "macos", not(target_arch = "wasm32")))]
        let default_backend = SugarloafBackend::Metal;

        SugarloafRenderer {
            power_preference: wgpu::PowerPreference::HighPerformance,
            backend: default_backend,
            font_features: None,
            colorspace: Colorspace::default(),
        }
    }
}

impl SugarloafWindow {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.handle
    }

    fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        self.display
    }
}

impl HasWindowHandle for SugarloafWindow {
    fn window_handle(&self) -> std::result::Result<WindowHandle<'_>, HandleError> {
        let raw = self.raw_window_handle();
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for SugarloafWindow {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        let raw = self.raw_display_handle();
        Ok(unsafe { DisplayHandle::borrow_raw(raw) })
    }
}

unsafe impl Send for SugarloafWindow {}
unsafe impl Sync for SugarloafWindow {}

impl Sugarloaf<'_> {
    pub fn new<'a>(
        window: SugarloafWindow,
        renderer: SugarloafRenderer,
        font_library: &FontLibrary,
        layout: RootStyle,
    ) -> Result<Sugarloaf<'a>, Box<SugarloafWithErrors<'a>>> {
        let font_features = renderer.font_features.to_owned();
        let colorspace = renderer.colorspace;
        let ctx = Context::new(window, renderer);

        let renderer = Renderer::new(&ctx, colorspace);
        let state = SugarState::new(layout, font_library, &font_features);

        let font_cache = FontCache::new();

        let instance = Sugarloaf {
            state,
            ctx,
            background_color: Some(wgpu::Color::BLACK),
            background_image: None,
            renderer,
            graphics: Graphics::default(),
            filters_brush: None,
            image_data: rustc_hash::FxHashMap::default(),
            cpu_cache: crate::renderer::cpu::CpuCache::new(),
            font_cache,
        };

        Ok(instance)
    }

    #[inline]
    pub fn update_font(&mut self, font_library: &FontLibrary) {
        tracing::info!("requested a font change");

        // Clear the global font data cache to ensure fonts are reloaded
        crate::font::clear_font_data_cache();

        // Clear the atlas to remove old font glyphs
        self.renderer.clear_atlas();
        // Cached tinted glyphs alias the old atlas coordinates — drop them.
        self.cpu_cache.clear();
        // Glyph resolutions point at the old font ids — drop them.
        self.font_cache.clear();

        self.state.reset();
        self.state.set_fonts(font_library, &mut self.renderer);
    }

    /// Look up a single glyph in the font cache without performing
    /// a fallback walk. Returns `None` if the entry is missing.
    /// Use this in the first pass of a multi-cell layout to identify
    /// cells that still need resolution.
    #[inline]
    pub fn try_glyph_cached(&self, ch: char, attrs: Attributes) -> Option<ResolvedGlyph> {
        self.font_cache.get(&(ch, attrs)).copied()
    }

    /// Resolve a single glyph, filling the cache on miss. Acquires
    /// the FontLibrary read lock once if needed.
    #[inline]
    pub fn resolve_glyph(&mut self, ch: char, attrs: Attributes) -> ResolvedGlyph {
        if let Some(cached) = self.font_cache.get(&(ch, attrs)) {
            return *cached;
        }
        let font_lib = self.state.content.font_library().clone();
        resolve_with(&mut self.font_cache, &font_lib, ch, attrs)
    }

    /// Horizontal advance in pixels for a single char rendered with
    /// `attrs` at `font_size`, using the same font fallback as
    /// `resolve_glyph`. Answered from the `FontCache` entry — no
    /// `content().build()` round trip.
    ///
    /// Returns `0.0` when the font library can't produce an advance
    /// (font id unregistered or SFNT parse failure) — the same shape
    /// an OS text engine returns for an unmapped glyph, so callers
    /// can sum widths without branching. The failure is cached as an
    /// `AdvanceInfo` with `units_per_em = 0` (which `scaled` already
    /// treats as 0), so repeated queries for the same char don't
    /// re-walk the font data on every frame.
    ///
    /// Lazy: the glyph cache keeps the advance `None` until the first
    /// `char_advance` call for this `(char, attrs)`, then fills it for
    /// the rest of the session (or until `update_font` swaps the font
    /// library and clears the cache). The terminal grid path only
    /// writes/reads `ResolvedGlyph::width` (cell count), so it never
    /// pays for the hmtx / upem lookup that `char_advance` performs
    /// on first sighting.
    ///
    /// Intended for proportional UI labels (tab titles, palette,
    /// hints). Per-char isolated advance: does NOT account for
    /// kerning, ligatures, or emoji cluster formation. Callers that
    /// need those must build the full text span and measure via
    /// `get_text_rendered_width`.
    pub fn char_advance(&mut self, ch: char, attrs: Attributes, font_size: f32) -> f32 {
        let resolved = self.resolve_glyph(ch, attrs);
        if let Some(advance) = resolved.advance {
            return advance.scaled(font_size);
        }

        let computed = {
            let font_ctx = self.state.content.font_library().inner.read();
            compute_advance(&font_ctx, resolved.font_id, ch)
        };
        // Cache both hits AND misses — misses become a zero-advance
        // sentinel (`units_per_em = 0`) so `scaled()` returns 0 and
        // next frame short-circuits instead of re-walking font data.
        let info = computed.unwrap_or(crate::font_cache::AdvanceInfo {
            advance_units: 0.0,
            units_per_em: 0,
        });
        self.font_cache.set_advance((ch, attrs), info);
        info.scaled(font_size)
    }

    /// Sorted, deduplicated family names of every font the host system
    /// exposes via `font-kit`'s `SystemSource`. Intended for UI listings
    /// (the command palette's "List Fonts" browser). Not cached — the
    /// set changes rarely, and the one-off cost of walking the library
    /// is fine for a human-triggered lookup.
    pub fn font_family_names(&self) -> Vec<String> {
        self.state.content.font_library().family_names()
    }

    /// Borrow the font library. Used by the grid emission path to
    /// resolve per-codepoint fonts before rasterizing into the grid's
    /// own atlas.
    #[inline]
    pub fn font_library(&self) -> &crate::font::FontLibrary {
        self.state.content.font_library()
    }

    /// Resolve a batch of glyph queries with a single FontLibrary
    /// read lock acquisition. Cache hits short-circuit; misses are
    /// walked under the lock and stored back in the cache. Returned
    /// vector is parallel to `queries`.
    #[inline]
    pub fn resolve_glyphs_batch(
        &mut self,
        queries: &[(char, Attributes)],
    ) -> Vec<ResolvedGlyph> {
        if queries.is_empty() {
            return Vec::new();
        }
        let font_lib = self.state.content.font_library().clone();
        let mut out = Vec::with_capacity(queries.len());
        for &(ch, attrs) in queries {
            out.push(resolve_with(&mut self.font_cache, &font_lib, ch, attrs));
        }
        out
    }

    #[inline]
    pub fn get_context(&self) -> &Context<'_> {
        &self.ctx
    }

    #[inline]
    pub fn get_scale(&self) -> f32 {
        self.ctx.scale()
    }

    #[inline]
    pub fn style(&self) -> RootStyle {
        self.state.style
    }

    #[inline]
    pub fn style_mut(&mut self) -> &mut RootStyle {
        &mut self.state.style
    }

    /// Update text font size based on action (0=reset, 1=decrease, 2=increase)
    /// Returns true if the operation was applied, false if id is not text
    #[inline]
    pub fn set_text_font_size_action(&mut self, id: &usize, operation: u8) -> bool {
        if self.state.content.get_text_by_id(*id).is_some() {
            self.state.update_text_style(id, operation);
            true
        } else {
            false
        }
    }

    /// Set font size for text content. Returns true if applied, false if id is not text
    #[inline]
    pub fn set_text_font_size(&mut self, id: &usize, font_size: f32) -> bool {
        if self.state.content.get_text_by_id(*id).is_some() {
            self.state.set_text_font_size(id, font_size);
            true
        } else {
            false
        }
    }

    /// Set line height for text content. Returns true if applied, false if id is not text
    #[inline]
    pub fn set_text_line_height(&mut self, id: &usize, line_height: f32) -> bool {
        if self.state.content.get_text_by_id(*id).is_some() {
            self.state.set_text_line_height(id, line_height);
            true
        } else {
            false
        }
    }

    #[inline]
    pub fn update_filters(&mut self, filters: &[Filter]) {
        if filters.is_empty() {
            self.filters_brush = None;
        } else {
            if self.filters_brush.is_none() {
                self.filters_brush = Some(FiltersBrush::default());
            }
            if let Some(ref mut brush) = self.filters_brush {
                if let crate::context::ContextType::Wgpu(ctx) = &self.ctx.inner {
                    brush.update_filters(ctx, filters);
                }
            }
        }
    }

    #[inline]
    pub fn set_background_color(&mut self, color: Option<wgpu::Color>) -> &mut Self {
        self.background_color = color;
        self
    }

    /// Try to load and install a window background image. Returns `Err`
    /// with a human-readable message on failure (file missing, decode
    /// failed, decoded image is empty, etc.) so callers can surface the
    /// message in a UI overlay. The decoded pixels are uploaded to a
    /// dedicated GPU texture sized to the image — the glyph atlas is not
    /// touched, so a 4K wallpaper does not push glyphs out of cache.
    #[inline]
    pub fn set_background_image(
        &mut self,
        image: &ImageProperties,
    ) -> Result<(), String> {
        // Skip if the same image is already configured. Both the path and
        // the opacity must match — opacity is baked into the alpha channel
        // at upload time, so an opacity change requires a reload.
        if let Some(current) = &self.background_image {
            if current.path == image.path && current.opacity == image.opacity {
                return Ok(());
            }
        }

        // Decode the file synchronously.
        let mut decoded = match image_rs::open(&image.path) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                let msg = format!("'{}': {}", image.path, e);
                tracing::warn!("failed to load background image {}", msg);
                return Err(msg);
            }
        };
        let (img_w, img_h) = decoded.dimensions();
        if img_w == 0 || img_h == 0 {
            let msg = format!("'{}' decoded to a {}x{} image", image.path, img_w, img_h);
            tracing::warn!("background image {}", msg);
            return Err(msg);
        }

        // Apply per-image opacity by scaling the alpha channel before
        // upload. The image fragment shader premultiplies alpha at sample
        // time, so the GPU does the right thing for both fully-opaque and
        // partially-translucent source images.
        let opacity = image.opacity.clamp(0.0, 1.0);
        if opacity < 1.0 {
            let opacity_byte = (opacity * 255.0).round() as u16;
            for pixel in decoded.pixels_mut() {
                pixel[3] = ((pixel[3] as u16 * opacity_byte) / 255) as u8;
            }
        }

        self.renderer.set_background_image_pixels(Some(
            crate::renderer::BackgroundImagePixels {
                width: img_w,
                height: img_h,
                pixels: decoded.into_raw(),
            },
        ));
        self.background_image = Some(image.clone());
        Ok(())
    }

    /// Drop the current background image, if any.
    #[inline]
    pub fn clear_background_image(&mut self) {
        if self.background_image.is_none() {
            return;
        }
        self.renderer.set_background_image_pixels(None);
        self.background_image = None;
    }

    /// Remove content by ID (any type)
    #[inline]
    pub fn remove_content(&mut self, id: usize) {
        self.state.content.remove_state(&id);
    }

    /// Clear text content (resets to empty). Returns true if applied, false if id is not text
    #[inline]
    pub fn clear_text(&mut self, id: &usize) -> bool {
        if self.state.content.get_text_by_id(*id).is_some() {
            self.state.clear_text(id);
            true
        } else {
            false
        }
    }

    pub fn content(&mut self) -> &mut Content {
        self.state.content()
    }

    #[inline]
    pub fn get_text_by_id_mut(
        &mut self,
        id: usize,
    ) -> Option<&mut crate::layout::BuilderState> {
        self.state.content.get_text_by_id_mut(id)
    }

    #[inline]
    pub fn get_text_by_id(&mut self, id: usize) -> Option<&crate::layout::BuilderState> {
        self.state.content.get_text_by_id(id)
    }

    #[inline]
    pub fn build_text_by_id(&mut self, id: usize) {
        self.state.content().sel(id).build();
    }

    #[inline]
    pub fn build_text_by_id_line_number(&mut self, text_id: usize, line_number: usize) {
        self.state.content().sel(text_id).build_line(line_number);
    }

    /// Create or get text content.
    /// - `id: Some(n)` - cached with id n, persistent across renders
    /// - `id: None` - transient text, cleared after rendering. Returns index into transient vec.
    #[inline]
    pub fn text(&mut self, id: Option<usize>) -> usize {
        match id {
            Some(text_id) => {
                // Check if text already exists
                if self.state.content.get_text_by_id(text_id).is_none() {
                    // Create new text with default layout
                    let default_layout =
                        TextLayout::from_default_layout(&self.state.style);
                    self.state.content.set_text(text_id, &default_layout);
                }
                text_id
            }
            None => {
                // Create transient text
                let default_layout = TextLayout::from_default_layout(&self.state.style);
                self.state.content.add_transient_text(&default_layout)
            }
        }
    }

    /// Get mutable reference to text content by id (for cached text)
    #[inline]
    pub fn get_text_mut(
        &mut self,
        id: usize,
    ) -> Option<&mut crate::layout::BuilderState> {
        self.state.content.get_text_by_id_mut(id)
    }

    /// Get mutable reference to transient text by index
    #[inline]
    pub fn get_transient_text_mut(
        &mut self,
        index: usize,
    ) -> Option<&mut crate::layout::BuilderState> {
        self.state.content.get_transient_text_mut(index)
    }

    /// Set font size for transient text
    #[inline]
    pub fn set_transient_text_font_size(&mut self, index: usize, font_size: f32) {
        if let Some(content_state) = self.state.content.get_transient_state_mut(index) {
            if let Some(text_state) = content_state.as_text_mut() {
                text_state.layout.font_size = font_size;
                text_state.scaled_font_size = font_size * self.state.style.scale_factor;
            }
            content_state.render_data.needs_repaint = true;
        }
    }

    /// Set position for transient text
    #[inline]
    pub fn set_transient_position(&mut self, index: usize, x: f32, y: f32) {
        if let Some(content_state) = self.state.content.get_transient_state_mut(index) {
            content_state.render_data.set_position(
                x * self.state.style.scale_factor,
                y * self.state.style.scale_factor,
            );
        }
    }

    /// Set visibility for transient text
    #[inline]
    pub fn set_transient_visibility(&mut self, index: usize, visible: bool) {
        if let Some(content_state) = self.state.content.get_transient_state_mut(index) {
            content_state.render_data.set_hidden(!visible);
        }
    }

    /// Set whether to use grid cell size for glyph positioning (cached text)
    /// - true: monospace grid alignment (default, for terminal)
    /// - false: proportional text using actual glyph advances (for rich text)
    #[inline]
    pub fn set_use_grid_cell_size(&mut self, id: usize, use_grid: bool) {
        if let Some(content_state) = self.state.content.states.get_mut(&id) {
            content_state.render_data.use_grid_cell_size = use_grid;
        }
    }

    /// Set the render order for a transient text element.
    #[inline]
    pub fn set_transient_order(&mut self, index: usize, order: u8) {
        if let Some(content_state) = self.state.content.get_transient_state_mut(index) {
            content_state.render_data.order = order;
        }
    }

    /// Set whether to use grid cell size for glyph positioning (transient text)
    /// - true: monospace grid alignment (default, for terminal)
    /// - false: proportional text using actual glyph advances (for rich text)
    #[inline]
    pub fn set_transient_use_grid_cell_size(&mut self, index: usize, use_grid: bool) {
        if let Some(content_state) = self.state.content.get_transient_state_mut(index) {
            content_state.render_data.use_grid_cell_size = use_grid;
        }
    }

    /// Get the next available ID for cached content.
    /// Returns the highest key + 1 (wrapping on overflow).
    /// Useful for dynamically allocating IDs without hardcoded constants.
    #[inline]
    pub fn get_next_id(&self) -> usize {
        self.state
            .content
            .states
            .keys()
            .max()
            .map(|max_id| max_id.wrapping_add(1))
            .unwrap_or(0)
    }

    /// Add a rectangle to content system
    /// - `id: None` - not cached, rendered immediately
    /// - `id: Some(n)` - cached with id n, overwrites existing content
    /// - `order` - draw order (higher values render on top)
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        &mut self,
        id: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        order: u8,
    ) {
        let scaled_x = x * self.state.style.scale_factor;
        let scaled_y = y * self.state.style.scale_factor;
        let scaled_width = width * self.state.style.scale_factor;
        let scaled_height = height * self.state.style.scale_factor;

        if let Some(content_id) = id {
            self.state.content.set_rect(
                content_id,
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height,
                color,
                depth,
            );
        } else {
            self.renderer.rect(
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height,
                color,
                depth,
                order,
            );
        }
    }

    /// Add a rounded rectangle to content system
    /// - `id: None` - not cached, rendered immediately
    /// - `id: Some(n)` - cached with id n, overwrites existing content
    /// - `order` - draw order (higher values render on top)
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn rounded_rect(
        &mut self,
        id: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
        border_radius: f32,
        order: u8,
    ) {
        let scaled_x = x * self.state.style.scale_factor;
        let scaled_y = y * self.state.style.scale_factor;
        let scaled_width = width * self.state.style.scale_factor;
        let scaled_height = height * self.state.style.scale_factor;
        let scaled_border_radius = border_radius * self.state.style.scale_factor;

        if let Some(content_id) = id {
            self.state.content.set_rounded_rect(
                content_id,
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height,
                color,
                depth,
                scaled_border_radius,
            );
        } else {
            self.renderer.rounded_rect(
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height,
                color,
                depth,
                scaled_border_radius,
                order,
            );
        }
    }

    /// Add a quad with per-corner radii and per-edge border widths
    /// - `id: None` - not cached, rendered immediately
    /// - `id: Some(n)` - cached with id n, overwrites existing content
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn quad(
        &mut self,
        _id: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        background_color: [f32; 4],
        corner_radii: [f32; 4],
        depth: f32,
        order: u8,
    ) {
        let scale = self.state.style.scale_factor;
        let scaled_x = x * scale;
        let scaled_y = y * scale;
        let scaled_width = width * scale;
        let scaled_height = height * scale;
        let scaled_corner_radii = [
            corner_radii[0] * scale,
            corner_radii[1] * scale,
            corner_radii[2] * scale,
            corner_radii[3] * scale,
        ];

        // For now, quad is always rendered immediately (no caching support yet)
        self.renderer.quad(
            scaled_x,
            scaled_y,
            scaled_width,
            scaled_height,
            background_color,
            scaled_corner_radii,
            depth,
            order,
        );
    }

    /// Add an image rectangle to content system
    /// - `id: None` - not cached, rendered immediately
    /// - `id: Some(n)` - cached with id n, overwrites existing content
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn image_rect(
        &mut self,
        id: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        coords: [f32; 4],
        depth: f32,
        atlas_layer: i32,
    ) {
        let scaled_x = x * self.state.style.scale_factor;
        let scaled_y = y * self.state.style.scale_factor;
        let scaled_width = width * self.state.style.scale_factor;
        let scaled_height = height * self.state.style.scale_factor;

        if let Some(content_id) = id {
            self.state.content.set_image(
                content_id,
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height,
                color,
                coords,
                depth,
                atlas_layer,
            );
        } else {
            self.renderer.add_image_rect(
                scaled_x,
                scaled_y,
                scaled_width,
                scaled_height,
                color,
                coords,
                depth,
                atlas_layer,
            );
        }
    }

    /// Draw an anti-aliased polygon from a list of points.
    /// Coordinates are in logical pixels (scaled internally).
    #[inline]
    pub fn polygon(&mut self, points: &[(f32, f32)], depth: f32, color: [f32; 4]) {
        let scale = self.state.style.scale_factor;
        let scaled: Vec<(f32, f32)> =
            points.iter().map(|(x, y)| (x * scale, y * scale)).collect();
        self.renderer.polygon(&scaled, depth, color);
    }

    /// Draw a triangle.
    /// Coordinates are in logical pixels (scaled internally).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn triangle(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x3: f32,
        y3: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        let s = self.state.style.scale_factor;
        self.renderer.triangle(
            x1 * s,
            y1 * s,
            x2 * s,
            y2 * s,
            x3 * s,
            y3 * s,
            depth,
            color,
        );
    }

    /// Draw a line between two points.
    /// Coordinates and width are in logical pixels (scaled internally).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn line(
        &mut self,
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        width: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        let s = self.state.style.scale_factor;
        self.renderer
            .line(x1 * s, y1 * s, x2 * s, y2 * s, width * s, depth, color);
    }

    /// Draw an arc (stroke only).
    /// Coordinates, radius, and stroke width are in logical pixels (scaled internally).
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        center_x: f32,
        center_y: f32,
        radius: f32,
        start_angle_deg: f32,
        end_angle_deg: f32,
        stroke_width: f32,
        depth: f32,
        color: [f32; 4],
    ) {
        let s = self.state.style.scale_factor;
        self.renderer.arc(
            center_x * s,
            center_y * s,
            radius * s,
            start_angle_deg,
            end_angle_deg,
            stroke_width * s,
            depth,
            color,
        );
    }

    /// Show content at a specific position (any type)
    #[inline]
    pub fn set_position(&mut self, id: usize, x: f32, y: f32) {
        self.state.set_content_position(id, x, y);
    }

    /// Set clipping bounds for content (physical pixels: [x, y, width, height])
    #[inline]
    pub fn set_bounds(&mut self, id: usize, bounds: Option<[f32; 4]>) {
        self.state.set_content_bounds(id, bounds);
    }

    /// Set content visibility (any type)
    #[inline]
    pub fn set_visibility(&mut self, id: usize, visible: bool) {
        self.state.set_content_hidden(id, !visible);
    }

    /// Set content depth for z-ordering
    #[inline]
    pub fn set_depth(&mut self, id: usize, depth: f32) {
        self.state.set_content_depth(id, depth);
    }

    /// Set content draw order (higher = drawn later = on top)
    #[inline]
    pub fn set_order(&mut self, id: usize, order: u8) {
        self.state.set_content_order(id, order);
    }

    /// Get text layout. Returns None if id is not text
    #[inline]
    pub fn get_text_layout(&self, id: &usize) -> Option<TextLayout> {
        self.state.content.get_text_by_id(*id)?;
        Some(self.state.get_state_layout(id))
    }

    /// Force update dimensions for text content
    #[inline]
    pub fn force_update_dimensions(&mut self, id: &usize) {
        self.state.content.update_dimensions(id);
    }

    /// Get text dimensions. Returns None if id is not text
    /// Get the total rendered width of text content by summing glyph advances.
    /// Returns the width in logical (unscaled) pixels.
    #[inline]
    pub fn get_text_rendered_width(&self, id: &usize) -> f32 {
        if let Some(builder_state) = self.state.content.get_text_by_id(*id) {
            let scale = self.state.style.scale_factor;
            let mut total: f32 = 0.0;
            for line in &builder_state.lines {
                for run in &line.render_data.runs {
                    total += run.advance;
                }
            }
            total / scale
        } else {
            0.0
        }
    }

    #[inline]
    pub fn get_text_dimensions(&mut self, id: &usize) -> Option<TextDimensions> {
        if self.state.content.get_text_by_id(*id).is_some() {
            Some(self.state.get_text_dimensions(id))
        } else {
            None
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.state.clean_screen();
    }

    #[inline]
    pub fn window_size(&self) -> SugarloafWindowSize {
        self.ctx.size()
    }

    #[inline]
    pub fn scale_factor(&self) -> f32 {
        self.state.style.scale_factor
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.renderer.resize(&mut self.ctx);
        // No content-state refresh needed for the background image — the
        // dedicated draw call reads `ctx.size` directly each frame.
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) {
        self.ctx.set_scale(scale);
        self.state.compute_layout_rescale(scale);
    }

    #[inline]
    pub fn add_layers(&mut self, _quantity: usize) {}

    #[inline]
    pub fn reset(&mut self) {
        self.state.reset();
    }

    #[inline]
    pub fn render(&mut self) {
        self.render_with_grids(&mut []);
    }

    /// Render variant that takes terminal grid renderers. Each grid's
    /// cell draws land inside the same render pass as sugarloaf's own
    /// UI overlays, so grid cells composite under island / assistant /
    /// etc. with a single drawable acquisition + present.
    ///
    /// Pass `&mut []` to skip (equivalent to `render()`). Phase 2 call
    /// sites in rioterm build the slice with one entry per panel.
    #[inline]
    pub fn render_with_grids(
        &mut self,
        grids: &mut [(&mut crate::grid::GridRenderer, crate::grid::GridUniforms)],
    ) {
        self.state.compute_dimensions();
        self.state.compute_updates(
            &mut self.renderer,
            &mut self.ctx,
            &mut self.graphics,
            &mut self.image_data,
        );

        match self.ctx.inner {
            crate::context::ContextType::Wgpu(_) => {
                self.render_wgpu(grids);
            }
            #[cfg(target_os = "macos")]
            crate::context::ContextType::Metal(_) => {
                self.render_metal(grids);
            }
            crate::context::ContextType::Cpu(_) => {
                self.render_cpu();
            }
        }
    }

    #[inline]
    pub fn render_cpu(&mut self) {
        let bg = self.background_color;
        let cpu_ctx = match &mut self.ctx.inner {
            crate::context::ContextType::Cpu(c) => c,
            _ => return,
        };

        crate::renderer::cpu::render_cpu(
            cpu_ctx,
            &self.renderer,
            &mut self.cpu_cache,
            bg,
        );

        self.reset();
    }

    /// Drive a Metal frame. All command-buffer / encoder / drawable
    /// orchestration now lives inside `Renderer::render_metal` so the
    /// triple-buffered pool's acquire / completion-handler / retry-on-
    /// overflow loop can see them all (mirrors zed's `MetalRenderer::draw`).
    #[inline]
    #[cfg(target_os = "macos")]
    pub fn render_metal(
        &mut self,
        grids: &mut [(&mut crate::grid::GridRenderer, crate::grid::GridUniforms)],
    ) {
        let ctx = match &mut self.ctx.inner {
            crate::context::ContextType::Metal(metal) => metal,
            _ => return,
        };

        let bg_color = self
            .background_color
            .map(|c| [c.r as f32, c.g as f32, c.b as f32, c.a as f32]);
        self.renderer.render_metal(ctx, bg_color, grids);

        self.reset();
    }

    #[inline]
    pub fn render_wgpu(
        &mut self,
        grids: &mut [(&mut crate::grid::GridRenderer, crate::grid::GridUniforms)],
    ) {
        let ctx = match &mut self.ctx.inner {
            crate::context::ContextType::Wgpu(wgpu) => wgpu,
            _ => return,
        };

        match ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder =
                    ctx.device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                            label: None,
                        });

                let view = frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                {
                    let load = if let Some(background_color) = self.background_color {
                        wgpu::LoadOp::Clear(background_color)
                    } else {
                        wgpu::LoadOp::Load
                    };

                    let mut rpass =
                        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            label: None,
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view: &view,
                                resolve_target: None,
                                depth_slice: None,
                                ops: wgpu::Operations {
                                    load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                            multiview_mask: None,
                        });

                    // Grid passes first — cell bg/text composite under
                    // the rich-text UI overlays drawn below.
                    for (grid, uniforms) in grids.iter_mut() {
                        grid.render_wgpu(&mut rpass, uniforms);
                    }

                    self.renderer.render(ctx, &mut rpass);
                }

                if let Some(ref mut filters_brush) = self.filters_brush {
                    filters_brush.render(
                        ctx,
                        &mut encoder,
                        &frame.texture,
                        &frame.texture,
                    );
                }
                ctx.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
        self.reset();
    }
}
