pub mod compositors;
pub mod graphics;
pub mod primitives;
pub mod state;

use crate::components::core::{image::Handle, shapes::Rectangle};
use crate::components::layer::{self, LayerBrush};
use crate::components::rect::{Rect, RectBrush};
use crate::components::rich_text::RichTextBrush;
use crate::components::text;
use crate::font::{fonts::SugarloafFont, FontLibrary};
use crate::layout::SugarloafLayout;
use crate::sugarloaf::graphics::{BottomLayer, Graphics};
use crate::sugarloaf::layer::types;
use crate::{context::Context, Object};
use ab_glyph::{self, PxScale};
use core::fmt::{Debug, Formatter};
use primitives::ImageProperties;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use state::SugarState;

pub struct Sugarloaf<'a> {
    pub ctx: Context<'a>,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    layer_brush: LayerBrush,
    rich_text_brush: RichTextBrush,
    state: state::SugarState,
    pub background_color: Option<wgpu::Color>,
    pub background_image: Option<ImageProperties>,
    pub graphics: Graphics,
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

pub struct SugarloafRenderer {
    pub power_preference: wgpu::PowerPreference,
    pub backend: wgpu::Backends,
    pub font_features: Option<Vec<String>>,
}

impl Default for SugarloafRenderer {
    fn default() -> SugarloafRenderer {
        #[cfg(target_arch = "wasm32")]
        let default_backend = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
        #[cfg(not(target_arch = "wasm32"))]
        let default_backend = wgpu::Backends::all();

        SugarloafRenderer {
            power_preference: wgpu::PowerPreference::HighPerformance,
            backend: default_backend,
            font_features: None,
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
    fn window_handle(&self) -> std::result::Result<WindowHandle, HandleError> {
        let raw = self.raw_window_handle();
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

impl HasDisplayHandle for SugarloafWindow {
    fn display_handle(&self) -> Result<DisplayHandle, HandleError> {
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
        layout: SugarloafLayout,
    ) -> Result<Sugarloaf<'a>, SugarloafWithErrors<'a>> {
        let font_features = renderer.font_features.to_owned();
        let ctx = Context::new(window, renderer);

        let text_brush = {
            let data = { font_library.inner.lock().ui.to_owned() };
            text::GlyphBrushBuilder::using_fonts(vec![data])
                .build(&ctx.device, ctx.format)
        };

        let rect_brush = RectBrush::init(&ctx);
        let layer_brush = LayerBrush::new(&ctx);
        let rich_text_brush = RichTextBrush::new(&ctx);
        let state = SugarState::new(layout, font_library, &font_features);

        let instance = Sugarloaf {
            state,
            layer_brush,
            ctx,
            background_color: Some(wgpu::Color::BLACK),
            background_image: None,
            rect_brush,
            rich_text_brush,
            text_brush,
            graphics: Graphics::default(),
        };

        Ok(instance)
    }

    #[inline]
    pub fn update_font(&mut self, font_library: &FontLibrary) {
        tracing::info!("requested a font change");

        self.state.reset_compositor();
        self.state.set_fonts(font_library);
    }

    #[inline]
    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    #[inline]
    pub fn get_scale(&self) -> f32 {
        self.ctx.scale
    }

    #[inline]
    pub fn layout(&self) -> SugarloafLayout {
        self.state.layout
    }

    #[inline]
    pub fn layout_mut(&mut self) -> &mut SugarloafLayout {
        &mut self.state.layout
    }

    #[inline]
    pub fn update_font_size(&mut self, operation: u8) {
        self.state.compute_layout_font_size(operation);
    }

    #[inline]
    pub fn set_background_color(&mut self, color: Option<wgpu::Color>) -> &mut Self {
        self.background_color = color;
        self
    }

    #[inline]
    pub fn set_background_image(&mut self, image: &ImageProperties) -> &mut Self {
        let handle = Handle::from_path(image.path.to_owned());
        self.graphics.bottom_layer = Some(BottomLayer {
            should_fit: image.width.is_none() && image.height.is_none(),
            data: types::Raster {
                handle,
                bounds: Rectangle {
                    width: image.width.unwrap_or(self.ctx.size.width),
                    height: image.height.unwrap_or(self.ctx.size.height),
                    x: image.x,
                    y: image.y,
                },
            },
        });
        self
    }

    #[inline]
    pub fn content(&mut self) -> &mut crate::Content {
        self.state.content()
    }

    #[inline]
    pub fn set_objects(&mut self, objects: Vec<Object>) {
        self.state.compute_objects(objects);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.state.clean_screen();
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.state.compute_layout_resize(width, height);
        if let Some(bottom_layer) = &mut self.graphics.bottom_layer {
            if bottom_layer.should_fit {
                bottom_layer.data.bounds.width = self.ctx.size.width;
                bottom_layer.data.bounds.height = self.ctx.size.height;
            }
        }
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) {
        self.ctx.scale = scale;
        self.state.compute_layout_rescale(scale);
        if let Some(bottom_layer) = &mut self.graphics.bottom_layer {
            if bottom_layer.should_fit {
                bottom_layer.data.bounds.width = self.ctx.size.width;
                bottom_layer.data.bounds.height = self.ctx.size.height;
            }
        }
    }

    #[inline]
    fn clean_state(&mut self) {
        self.state.clean_compositor();
    }

    #[inline]
    pub fn render(&mut self) {
        self.state.compute_changes();
        self.state.compute_dimensions(&mut self.rich_text_brush);

        if !self.state.compute_updates(
            &mut self.rich_text_brush,
            &mut self.text_brush,
            &mut self.rect_brush,
            &mut self.ctx,
            &mut self.graphics,
        ) {
            self.clean_state();
            return;
        }

        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                if let Some(layer) = &self.graphics.bottom_layer {
                    self.layer_brush
                        .prepare(&mut encoder, &mut self.ctx, &[&layer.data]);
                }

                if self.graphics.has_graphics_on_top_layer() {
                    for request in &self.graphics.top_layer {
                        if let Some(entry) = self.graphics.get(&request.id) {
                            self.layer_brush.prepare_with_handle(
                                &mut encoder,
                                &mut self.ctx,
                                &entry.handle,
                                &Rectangle {
                                    width: request.width.unwrap_or(entry.width),
                                    height: request.height.unwrap_or(entry.height),
                                    x: request.pos_x,
                                    y: request.pos_y,
                                },
                            );
                        }
                    }
                }

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
                                view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load,
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                        });

                    if self.graphics.bottom_layer.is_some() {
                        self.layer_brush.render(0, &mut rpass, None);
                    }

                    if self.graphics.has_graphics_on_top_layer() {
                        let range_request = if self.graphics.bottom_layer.is_some() {
                            1..(self.graphics.top_layer.len() + 1)
                        } else {
                            0..self.graphics.top_layer.len()
                        };
                        for request in range_request {
                            self.layer_brush.render(request, &mut rpass, None);
                        }
                    }

                    self.rich_text_brush
                        .render(&mut self.ctx, &self.state, &mut rpass);

                    self.rect_brush
                        .render(&mut rpass, &self.state, &mut self.ctx);

                    self.text_brush.render(&mut self.ctx, &mut rpass);
                }

                if self.graphics.bottom_layer.is_some()
                    || self.graphics.has_graphics_on_top_layer()
                {
                    self.layer_brush.end_frame();
                    self.graphics.clear_top_layer();
                }

                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
        self.clean_state();
    }
}
