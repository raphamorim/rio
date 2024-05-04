pub mod compositors;
pub mod graphics;
pub mod primitives;
pub mod state;
mod tree;

use crate::components::core::{image::Handle, shapes::Rectangle};
use crate::components::layer::{self, LayerBrush};
use crate::components::rect::{Rect, RectBrush};
use crate::components::rich_text::RichTextBrush;
use crate::components::text;
use crate::context::Context;
use crate::font::fonts::{SugarloafFont, SugarloafFonts};
#[cfg(not(target_arch = "wasm32"))]
use crate::font::loader::Database;
use crate::font::FontLibrary;
use crate::layout::SugarloafLayout;
use crate::sugarloaf::layer::types;
use crate::Sugar;
use crate::{SugarBlock, SugarText};
use ab_glyph::{self, PxScale};
use core::fmt::{Debug, Formatter};
use primitives::ImageProperties;
use raw_window_handle::{
    DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
};
use state::SugarState;

#[cfg(target_arch = "wasm32")]
pub struct Database;

pub struct Sugarloaf {
    pub ctx: Context,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    layer_brush: LayerBrush,
    rich_text_brush: RichTextBrush,
    state: state::SugarState,
    pub background_color: wgpu::Color,
    pub background_image: Option<types::Image>,
}

#[derive(Debug)]
pub struct SugarloafErrors {
    pub fonts_not_found: Vec<SugarloafFont>,
}

pub struct SugarloafWithErrors {
    pub instance: Sugarloaf,
    pub errors: SugarloafErrors,
}

impl Debug for SugarloafWithErrors {
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

impl Sugarloaf {
    pub async fn new(
        window: SugarloafWindow,
        renderer: SugarloafRenderer,
        font_library: &FontLibrary,
        layout: SugarloafLayout,
    ) -> Result<Sugarloaf, SugarloafWithErrors> {
        let ctx = Context::new(window, &renderer).await;
        // let mut sugarloaf_errors = None;

        // let (font_library, fonts_not_found) = loader;

        // if !fonts_not_found.is_empty() {
        //     sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        // }

        let data = { &font_library.inner.read().unwrap().main };

        let text_brush = text::GlyphBrushBuilder::using_fonts(vec![data.to_owned()])
            .build(&ctx.device, ctx.format);
        let rect_brush = RectBrush::init(&ctx);
        let layer_brush = LayerBrush::new(&ctx);
        let rich_text_brush = RichTextBrush::new(&ctx);

        let state = SugarState::new(layout, font_library);

        let instance = Sugarloaf {
            state,
            layer_brush,
            ctx,
            background_color: wgpu::Color::BLACK,
            background_image: None,
            rect_brush,
            rich_text_brush,
            text_brush,
        };

        // if let Some(errors) = sugarloaf_errors {
        //     return Err(SugarloafWithErrors { instance, errors });
        // }

        Ok(instance)
    }

    #[inline]
    pub fn update_font(
        &mut self,
        _fonts: SugarloafFonts,
        #[allow(unused)] db: Option<&Database>,
    ) -> Option<SugarloafErrors> {
        // if self.fonts != fonts {
        //     log::info!("requested a font change");

        //     #[cfg(not(target_arch = "wasm32"))]
        //     let loader = Font::load(fonts.to_owned(), db);
        //     #[cfg(target_arch = "wasm32")]
        //     let loader = Font::load(fonts.to_owned());

        //     let (font_library, fonts_not_found) = loader;
        //     if !fonts_not_found.is_empty() {
        //         return Some(SugarloafErrors { fonts_not_found });
        //     }

        //     let text_brush =
        //         text::GlyphBrushBuilder::using_fonts(font_library.font_arcs())
        //             .build(&self.ctx.device, self.ctx.format);

        //     self.text_brush = text_brush;
        //     self.fonts = fonts;

        //     self.state.reset_compositor();
        //     self.state.set_fonts(font_library);
        // }

        None
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
    pub fn layout_next(&self) -> SugarloafLayout {
        self.state.next.layout
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
    pub fn append_rects(&mut self, rects: Vec<Rect>) {
        self.state.compute_block(SugarBlock { rects, text: None });
    }

    #[inline]
    pub fn start_line(&mut self) {
        self.state.compute_line_start();
    }

    #[inline]
    pub fn insert_on_current_line(&mut self, sugar: &Sugar) {
        self.state.insert_on_current_line(sugar);
    }

    #[inline]
    pub fn insert_on_current_line_from_vec(&mut self, sugar_vec: &Vec<&Sugar>) {
        self.state.insert_on_current_line_from_vec(sugar_vec);
    }

    #[inline]
    pub fn insert_on_current_line_from_vec_owned(&mut self, sugar_vec: &Vec<Sugar>) {
        self.state.insert_on_current_line_from_vec_owned(sugar_vec);
    }

    #[inline]
    pub fn finish_line(&mut self) {
        self.state.compute_line_end();
    }

    #[inline]
    pub fn text(
        &mut self,
        position: (f32, f32),
        content: String,
        font_size: f32,
        color: [f32; 4],
        single_line: bool,
    ) {
        self.state.compute_block(SugarBlock {
            rects: vec![],
            text: Some(SugarText {
                position,
                content,
                font_id: 0,
                font_size,
                color,
                single_line,
            }),
        });
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
    pub fn add_graphic(&mut self, graphic: crate::SugarGraphicData) {
        self.state.graphics.add(graphic);
    }

    #[inline]
    pub fn remove_graphic(&mut self, graphic_id: &crate::SugarGraphicId) {
        self.state.graphics.remove(graphic_id);
    }

    #[inline]
    pub fn dimensions_changed(&self) -> bool {
        self.state.dimensions_changed()
    }

    #[inline]
    pub fn render(&mut self) {
        // let start = std::time::Instant::now();

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

        // let duration = start.elapsed();
        // println!(
        //     "Time elapsed in rich_text_brush.prepare() is: {:?} \n",
        //     duration
        // );

        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                // let load = if self.state.layout_was_updated() {
                //     wgpu::LoadOp::Clear(self.background_color)
                // } else {
                //     wgpu::LoadOp::Load
                // };
                if let Some(bg_image) = &self.background_image {
                    self.layer_brush.prepare_ref(
                        &mut encoder,
                        &mut self.ctx,
                        &[bg_image],
                    );
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

                    if self.background_image.is_some() {
                        self.layer_brush.render(0, &mut rpass, None);
                    }

                    self.rect_brush
                        .render(&mut rpass, &self.state, &mut self.ctx);

                    self.text_brush.render(&mut self.ctx, &mut rpass);

                    self.rich_text_brush
                        .render(&mut self.ctx, &self.state, &mut rpass);

                    // if !self.graphic_rects.is_empty() {
                    //     for entry_render in
                    //         &self.graphic_rects.keys().cloned().collect::<Vec<_>>()
                    //     {
                    //         if let Some(entry) = self.graphic_rects.get(entry_render) {
                    //             if let Some(graphic_data) = self.graphics.get(&entry.id) {
                    //                 let rows = entry.end_row - entry.start_row;
                    //                 let height = (rows - 2.) * self.layout.dimensions.height;

                    //                 let a = layer::types::Image::Raster {
                    //                     handle: graphic_data.handle.clone(),
                    //                     bounds: Rectangle {
                    //                         x: entry.pos_x,
                    //                         y: entry.pos_y,
                    //                         width: entry.width as f32,
                    //                         height,
                    //                     },
                    //                 };

                    //                 self.layer_brush.prepare_ref(
                    //                     &mut encoder,
                    //                     &mut self.ctx,
                    //                     &[&a],
                    //                 );

                    //                 self.layer_brush.render_with_encoder(
                    //                     0,
                    //                     view,
                    //                     &mut encoder,
                    //                     None,
                    //                 );
                    //             }
                    //         }
                    //     }
                    // }
                    // self.layer_brush.end_frame();
                }

                if self.background_image.is_some() {
                    self.layer_brush.end_frame();
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
