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
use crate::sugarloaf::graphics::{GraphicData, GraphicId};
use crate::sugarloaf::layer::types;
use crate::{context::Context, Content, Object};
use ab_glyph::{self, PxScale};
use core::fmt::{Debug, Formatter};
use primitives::ImageProperties;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use rustc_hash::FxHashMap;
use state::SugarState;

pub struct GraphicDataEntry {
    handle: Handle,
    width: f32,
    height: f32,
}

pub struct Sugarloaf<'a> {
    pub ctx: Context<'a>,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    layer_brush: LayerBrush,
    rich_text_brush: RichTextBrush,
    state: state::SugarState,
    pub background_color: wgpu::Color,
    pub background_image: Option<types::Image>,
    graphics: FxHashMap<GraphicId, GraphicDataEntry>,
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
            let data = { &font_library.inner.read().unwrap().main };
            text::GlyphBrushBuilder::using_fonts(vec![data.to_owned()])
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
            background_color: wgpu::Color::BLACK,
            background_image: None,
            rect_brush,
            rich_text_brush,
            text_brush,
            graphics: FxHashMap::default(),
        };

        Ok(instance)
    }

    #[inline]
    pub fn update_font(&mut self, font_library: &FontLibrary) {
        log::info!("requested a font change");

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
        self.state.current.layout
    }

    #[inline]
    pub fn layout_mut(&mut self) -> &mut SugarloafLayout {
        self.state.mark_dirty();
        &mut self.state.current.layout
    }

    #[inline]
    pub fn update_font_size(&mut self, operation: u8) {
        self.state.compute_layout_font_size(operation);
    }

    #[inline]
    pub fn set_background_color(&mut self, color: wgpu::Color) -> &mut Self {
        self.background_color = color;
        self
    }

    #[inline]
    pub fn set_background_image(&mut self, image: &ImageProperties) -> &mut Self {
        let handle = Handle::from_path(image.path.to_owned());
        self.background_image = Some(layer::types::Image::Raster {
            handle,
            bounds: Rectangle {
                width: image.width,
                height: image.height,
                x: image.x,
                y: image.y,
            },
        });
        self
    }

    #[inline]
    pub fn set_objects(&mut self, objects: Vec<Object>) {
        self.state.compute_objects(objects);
    }

    #[inline]
    pub fn set_content(&mut self, content: Content) {
        self.state.set_content(content);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.state.clean_screen();
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.state.compute_layout_resize(width, height);
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) {
        self.ctx.scale = scale;
        self.state.compute_layout_rescale(scale);
    }

    #[inline]
    fn clean_state(&mut self) {
        self.state.clean_compositor();
    }

    #[inline]
    pub fn add_graphic(&mut self, graphic_data: GraphicData) {
        if self.graphics.contains_key(&graphic_data.id) {
            return;
        }

        self.graphics.insert(
            graphic_data.id,
            GraphicDataEntry {
                handle: Handle::from_pixels(
                    graphic_data.width as u32,
                    graphic_data.height as u32,
                    graphic_data.pixels,
                ),
                width: graphic_data.width as f32,
                height: graphic_data.height as f32,
            },
        );
    }

    #[inline]
    pub fn remove_graphic(&mut self, graphic_id: &GraphicId) {
        self.graphics.remove(graphic_id);
    }

    #[inline]
    pub fn mark_dirty(&mut self) {
        self.state.mark_dirty();
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

                // if let Some(bg_image) = &self.background_image {
                //     self.layer_brush.prepare_ref(
                //         &mut encoder,
                //         &mut self.ctx,
                //         &[bg_image],
                //     );
                // }

                let graphic_requests = self.rich_text_brush.render_media_requests.len();
                if graphic_requests > 0 {
                    for request in &self.rich_text_brush.render_media_requests {
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
                    let mut rpass =
                        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                            timestamp_writes: None,
                            occlusion_query_set: None,
                            label: None,
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                                view,
                                resolve_target: None,
                                ops: wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(self.background_color),
                                    store: wgpu::StoreOp::Store,
                                },
                            })],
                            depth_stencil_attachment: None,
                        });

                    self.rich_text_brush
                        .render(&mut self.ctx, &self.state, &mut rpass);

                    for request in 0..graphic_requests {
                        self.layer_brush.render(request, &mut rpass, None);
                    }

                    self.rect_brush
                        .render(&mut rpass, &self.state, &mut self.ctx);

                    self.text_brush.render(&mut self.ctx, &mut rpass);
                }

                // if self.background_image.is_some() {
                if !self.rich_text_brush.render_media_requests.is_empty() {
                    self.layer_brush.end_frame();
                }
                // }

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
