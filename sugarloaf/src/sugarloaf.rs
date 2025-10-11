pub mod graphics;
pub mod primitives;
pub mod state;

use crate::components::core::{image::Handle, shapes::Rectangle};
use crate::components::filters::{Filter, FiltersBrush};
use crate::components::layer::{self, LayerBrush};
use crate::components::quad::QuadBrush;
use crate::components::rich_text::RichTextBrush;
use crate::font::{fonts::SugarloafFont, FontLibrary};
use crate::layout::{RichTextLayout, RootStyle};
use crate::sugarloaf::graphics::{BottomLayer, Graphics};
use crate::sugarloaf::layer::types;
use crate::Content;
use crate::SugarDimensions;
use crate::{context::Context, Object, Quad};
use core::fmt::{Debug, Formatter};
use primitives::ImageProperties;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use state::SugarState;

pub struct Sugarloaf<'a> {
    pub ctx: Context<'a>,
    quad_brush: QuadBrush,
    rich_text_brush: RichTextBrush,
    // layer_brush: LayerBrush,
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

        // let layer_brush = LayerBrush::new(&ctx);
        let quad_brush = QuadBrush::new(&ctx);
        let rich_text_brush = RichTextBrush::new(&ctx);
        let state = SugarState::new(layout, font_library, &font_features);

        let instance = Sugarloaf {
            state,
            // layer_brush,
            quad_brush,
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

        // Clear the layer atlas to remove old cached images
        // self.layer_brush.clear_atlas(
        //     &self.ctx.device,
        //     self.ctx.adapter_info.backend,
        //     &self.ctx,
        // );

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
        self.state.set_rich_text_font_size_based_on_action(
            rt_id,
            operation,
            &mut self.rich_text_brush,
        );
    }

    #[inline]
    pub fn set_rich_text_font_size(&mut self, rt_id: &usize, font_size: f32) {
        self.state
            .set_rich_text_font_size(rt_id, font_size, &mut self.rich_text_brush);
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
    pub fn set_background_image(&mut self, image: &ImageProperties) -> &mut Self {
        let handle = Handle::from_path(image.path.to_owned());
        // self.graphics.bottom_layer = Some(BottomLayer {
        //     should_fit: image.width.is_none() && image.height.is_none(),
        //     data: types::Raster {
        //         handle,
        //         bounds: Rectangle {
        //             width: image.width.unwrap_or(self.ctx.size.width),
        //             height: image.height.unwrap_or(self.ctx.size.height),
        //             x: image.x,
        //             y: image.y,
        //         },
        //     },
        // });
        self
    }

    #[inline]
    pub fn create_rich_text(&mut self) -> usize {
        self.state.create_rich_text()
    }

    #[inline]
    pub fn remove_rich_text(&mut self, rich_text_id: usize) {
        self.state.content.remove_state(&rich_text_id);
    }

    // This RichText is different than regular rich text
    // it will be removed after the render and doesn't
    // offer any type of optimization (e.g: cache) per render.
    #[inline]
    pub fn create_temp_rich_text(&mut self) -> usize {
        self.state.create_temp_rich_text()
    }

    #[inline]
    pub fn clear_rich_text(&mut self, id: &usize) {
        self.state.clear_rich_text(id);
    }

    pub fn content(&mut self) -> &mut Content {
        self.state.content()
    }

    #[inline]
    pub fn set_objects(&mut self, objects: Vec<Object>) {
        self.state.compute_objects(objects);
    }

    #[inline]
    pub fn rich_text_layout(&self, id: &usize) -> RichTextLayout {
        self.state.get_state_layout(id)
    }

    #[inline]
    pub fn get_rich_text_dimensions(&mut self, id: &usize) -> SugarDimensions {
        self.state
            .get_rich_text_dimensions(id, &mut self.rich_text_brush)
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
        // if let Some(bottom_layer) = &mut self.graphics.bottom_layer {
        //     if bottom_layer.should_fit {
        //         bottom_layer.data.bounds.width = self.ctx.size.width;
        //         bottom_layer.data.bounds.height = self.ctx.size.height;
        //     }
        // }
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) {
        self.ctx.set_scale(scale);
        self.state
            .compute_layout_rescale(scale, &mut self.rich_text_brush);
        // if let Some(bottom_layer) = &mut self.graphics.bottom_layer {
        //     if bottom_layer.should_fit {
        //         bottom_layer.data.bounds.width = self.ctx.size.width;
        //         bottom_layer.data.bounds.height = self.ctx.size.height;
        //     }
        // }
    }

    #[inline]
    pub fn add_layers(&mut self, _quantity: usize) {}

    #[inline]
    pub fn reset(&mut self) {
        self.state.reset();
    }

    #[inline]
    pub fn render(&mut self) {
        self.state.compute_dimensions(&mut self.rich_text_brush);
        self.state.compute_updates(
            &mut self.rich_text_brush,
            &mut self.quad_brush,
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

                self.quad_brush
                    .render_metal(ctx, &self.state, &render_encoder);
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
                // if let Some(layer) = &self.graphics.bottom_layer {
                //     self.layer_brush.prepare(&mut encoder, ctx, &[&layer.data]);
                // }

                // if self.graphics.has_graphics_on_top_layer() {
                //     for request in &self.graphics.top_layer {
                //         if let Some(entry) = self.graphics.get(&request.id) {
                //             self.layer_brush.prepare_with_handle(
                //                 &mut encoder,
                //                 ctx,
                //                 &entry.handle,
                //                 &Rectangle {
                //                     width: request.width.unwrap_or(entry.width),
                //                     height: request.height.unwrap_or(entry.height),
                //                     x: request.pos_x,
                //                     y: request.pos_y,
                //                 },
                //             );
                //         }
                //     }
                // }

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
                                ops: wgpu::Operations {
                                    load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                        });

                    // if self.graphics.bottom_layer.is_some() {
                    //     self.layer_brush.render(0, &mut rpass, None);
                    // }

                    // if self.graphics.has_graphics_on_top_layer() {
                    //     let range_request = if self.graphics.bottom_layer.is_some() {
                    //         1..(self.graphics.top_layer.len() + 1)
                    //     } else {
                    //         0..self.graphics.top_layer.len()
                    //     };
                    //     for request in range_request {
                    //         self.layer_brush.render(request, &mut rpass, None);
                    //     }
                    // }
                    self.quad_brush.render_wgpu(ctx, &self.state, &mut rpass);
                    self.rich_text_brush.render(ctx, &mut rpass);
                }

                // if self.graphics.bottom_layer.is_some()
                //     || self.graphics.has_graphics_on_top_layer()
                // {
                //     self.layer_brush.end_frame();
                //     self.graphics.clear_top_layer();
                // }

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
