pub mod graphics;
pub mod primitives;
pub mod state;

use crate::components::core::image::Handle;
use crate::components::filters::{Filter, FiltersBrush};
use crate::components::rich_text::RichTextBrush;
use crate::font::{fonts::SugarloafFont, FontLibrary};
use crate::layout::{RichTextConfig, RichTextLayout, RootStyle};
use crate::sugarloaf::graphics::Graphics;

use crate::context::Context;
use crate::Content;
use crate::SugarDimensions;
use core::fmt::{Debug, Formatter};
use primitives::ImageProperties;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use state::SugarState;

pub struct Sugarloaf<'a> {
    pub ctx: Context<'a>,
    rich_text_brush: RichTextBrush,
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

        let rich_text_brush = RichTextBrush::new(&ctx);
        let state = SugarState::new(layout, font_library, &font_features);

        let instance = Sugarloaf {
            state,
            ctx,
            background_color: Some(wgpu::Color::BLACK),
            background_image: None,
            rich_text_brush,
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
        self.rich_text_brush.clear_atlas();

        self.state.reset();
        self.state
            .set_fonts(font_library, &mut self.rich_text_brush);
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

    #[inline]
    pub fn set_rich_text_font_size_based_on_action(
        &mut self,
        rt_id: &usize,
        operation: u8,
    ) {
        self.state
            .set_rich_text_font_size_based_on_action(rt_id, operation);
    }

    #[inline]
    pub fn set_rich_text_font_size(&mut self, rt_id: &usize, font_size: f32) {
        self.state.set_rich_text_font_size(rt_id, font_size);
    }

    #[inline]
    pub fn set_rich_text_line_height(&mut self, rt_id: &usize, line_height: f32) {
        self.state.set_rich_text_line_height(rt_id, line_height);
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

    #[inline]
    pub fn create_rich_text(&mut self, config: Option<&RichTextConfig>) -> usize {
        self.state.create_rich_text(config)
    }

    #[inline]
    pub fn remove_rich_text(&mut self, rich_text_id: usize) {
        self.state.content.remove_state(&rich_text_id);
    }

    // This RichText is different than regular rich text
    // it will be removed after the render and doesn't
    // offer any type of optimization (e.g: cache) per render.
    #[inline]
    pub fn create_temp_rich_text(&mut self, config: Option<&RichTextConfig>) -> usize {
        self.state.create_temp_rich_text(config)
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize) {
        self.state.clear_rich_text(id);
    }

    pub fn content(&mut self) -> &mut Content {
        self.state.content()
    }

    /// Add a rectangle directly to the rendering pipeline
    #[inline]
    pub fn rect(
        &mut self,
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
        self.rich_text_brush.rect(
            scaled_x,
            scaled_y,
            scaled_width,
            scaled_height,
            color,
            depth,
        );
    }

    /// Add a rounded rectangle directly to the rendering pipeline
    #[inline]
    pub fn rounded_rect(
        &mut self,
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
        self.rich_text_brush.rounded_rect(
            scaled_x,
            scaled_y,
            scaled_width,
            scaled_height,
            color,
            depth,
            scaled_border_radius,
        );
    }

    /// Add an image rectangle directly to the rendering pipeline
    #[inline]
    pub fn add_image_rect(
        &mut self,
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
        self.rich_text_brush.add_image_rect(
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

    /// Show a rich text at a specific position
    #[inline]
    pub fn show_rich_text(&mut self, id: usize, x: f32, y: f32) {
        self.state
            .set_rich_text_visibility_and_position(id, x, y, false);
    }

    /// Hide a rich text
    #[inline]
    pub fn hide_rich_text(&mut self, id: usize) {
        self.state.set_rich_text_hidden(id, true);
    }

    /// Show/hide a rich text
    #[inline]
    pub fn set_rich_text_visibility(&mut self, id: usize, hidden: bool) {
        self.state.set_rich_text_hidden(id, hidden);
    }

    #[inline]
    pub fn rich_text_layout(&self, id: &usize) -> RichTextLayout {
        self.state.get_state_layout(id)
    }

    #[inline]
    pub fn force_update_dimensions(&mut self, id: &usize) {
        self.state.content.update_dimensions(id);
    }

    #[inline]
    pub fn get_rich_text_dimensions(&mut self, id: &usize) -> SugarDimensions {
        self.state.get_rich_text_dimensions(id)
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
        self.state.compute_updates(
            &mut self.rich_text_brush,
            &mut self.ctx,
            &mut self.graphics,
        );

        match self.ctx.inner {
            crate::context::ContextType::Wgpu(_) => {
                self.render_wgpu();
            }
            crate::context::ContextType::Metal(_) => {
                self.render_metal();
            }
        }
    }

    #[inline]
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
                    command_buffer.new_render_command_encoder(&render_pass_descriptor);
                render_encoder.set_label("Sugarloaf Metal Render Pass");

                self.rich_text_brush.render_metal(ctx, &render_encoder);

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
        let ctx = match &mut self.ctx.inner {
            crate::context::ContextType::Wgpu(wgpu) => wgpu,
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

                    self.rich_text_brush.render(ctx, &mut rpass);
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
