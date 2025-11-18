pub mod graphics;
pub mod primitives;
pub mod state;

use crate::components::core::image::Handle;
use crate::components::filters::{Filter, FiltersBrush};
use crate::font::{fonts::SugarloafFont, FontLibrary};
use crate::layout::{RootStyle, TextLayout};
use crate::renderer::Renderer;
use crate::sugarloaf::graphics::Graphics;

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

#[cfg(target_os = "macos")]
#[allow(clippy::derivable_impls)]
impl Default for Colorspace {
    fn default() -> Colorspace {
        Colorspace::DisplayP3
    }
}

#[cfg(not(target_os = "macos"))]
#[allow(clippy::derivable_impls)]
impl Default for Colorspace {
    fn default() -> Colorspace {
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
        let ctx = Context::new(window, renderer);

        let renderer = Renderer::new(&ctx);
        let state = SugarState::new(layout, font_library, &font_features);

        let instance = Sugarloaf {
            state,
            ctx,
            background_color: Some(wgpu::Color::BLACK),
            background_image: None,
            renderer,
            graphics: Graphics::default(),
            filters_brush: None,
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

        self.state.reset();
        self.state.set_fonts(font_library, &mut self.renderer);
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
                match &self.ctx.inner {
                    crate::context::ContextType::Wgpu(ctx) => {
                        brush.update_filters(ctx, filters);
                    }
                    #[cfg(target_os = "macos")]
                    _ => {}
                };
            }
        }
    }

    #[inline]
    pub fn set_background_color(&mut self, color: Option<wgpu::Color>) -> &mut Self {
        self.background_color = color;
        self
    }

    #[inline]
    pub fn set_background_image(&mut self, _image: &ImageProperties) -> &mut Self {
        // TODO: Background images are not yet implemented with the new rendering system
        self
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
    #[inline]
    pub fn rect(
        &mut self,
        id: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        depth: f32,
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
            );
        }
    }

    /// Add a rounded rectangle to content system
    /// - `id: None` - not cached, rendered immediately
    /// - `id: Some(n)` - cached with id n, overwrites existing content
    #[inline]
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
            );
        }
    }

    /// Add an image rectangle to content system
    /// - `id: None` - not cached, rendered immediately
    /// - `id: Some(n)` - cached with id n, overwrites existing content
    #[inline]
    pub fn image_rect(
        &mut self,
        id: Option<usize>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        color: [f32; 4],
        coords: [f32; 4],
        has_alpha: bool,
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
                has_alpha,
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
                has_alpha,
                depth,
                atlas_layer,
            );
        }
    }

    /// Show content at a specific position (any type)
    #[inline]
    pub fn set_position(&mut self, id: usize, x: f32, y: f32) {
        self.state.set_content_position(id, x, y);
    }

    /// Set content visibility (any type)
    #[inline]
    pub fn set_visibility(&mut self, id: usize, visible: bool) {
        self.state.set_content_hidden(id, !visible);
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
        self.state.compute_dimensions();
        self.state
            .compute_updates(&mut self.renderer, &mut self.ctx, &mut self.graphics);

        match self.ctx.inner {
            crate::context::ContextType::Wgpu(_) => {
                self.render_wgpu();
            }
            #[cfg(target_os = "macos")]
            crate::context::ContextType::Metal(_) => {
                self.render_metal();
            }
        }
    }

    #[inline]
    #[cfg(target_os = "macos")]
    pub fn render_metal(&mut self) {
        use metal::*;

        let ctx = match &mut self.ctx.inner {
            crate::context::ContextType::Metal(metal) => metal,
            crate::context::ContextType::Wgpu(_) => {
                return;
            }
        };

        match ctx.get_current_texture() {
            Ok(surface_texture) => {
                // Create command buffer
                let command_buffer = ctx.command_queue.new_command_buffer();
                command_buffer.set_label("Sugarloaf Metal Render");

                // Create render pass descriptor
                let render_pass_descriptor = RenderPassDescriptor::new();
                let color_attachment = render_pass_descriptor
                    .color_attachments()
                    .object_at(0)
                    .unwrap();

                color_attachment.set_texture(Some(&surface_texture.texture));
                color_attachment.set_store_action(MTLStoreAction::Store);
                color_attachment.set_load_action(MTLLoadAction::Clear);

                // Set background color
                let clear_color = if let Some(background_color) = self.background_color {
                    MTLClearColor::new(
                        background_color.r,
                        background_color.g,
                        background_color.b,
                        background_color.a,
                    )
                } else {
                    // Default to transparent black if no background color set
                    MTLClearColor::new(0.0, 0.0, 0.0, 0.0)
                };
                color_attachment.set_clear_color(clear_color);

                // Create render command encoder
                let render_encoder =
                    command_buffer.new_render_command_encoder(render_pass_descriptor);
                render_encoder.set_label("Sugarloaf Metal Render Pass");

                self.renderer.render_metal(ctx, render_encoder);

                render_encoder.end_encoding();
                command_buffer.present_drawable(&surface_texture.drawable);
                command_buffer.commit();
            }
            Err(error) => {
                tracing::error!("Metal surface error: {}", error);
            }
        }

        self.reset();
    }

    #[inline]
    pub fn render_wgpu(&mut self) {
        #[cfg_attr(
            not(target_os = "macos"),
            expect(clippy::infallible_destructuring_match)
        )]
        let ctx = match &mut self.ctx.inner {
            crate::context::ContextType::Wgpu(wgpu) => wgpu,
            #[cfg(target_os = "macos")]
            crate::context::ContextType::Metal(_) => {
                return;
            }
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
                        });

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
