use crate::components::core::{image::Handle, shapes::Rectangle};
use crate::components::layer::{self, LayerBrush};
use crate::components::rect::{Rect, RectBrush};
use crate::components::text;
use crate::context::Context;
use crate::core::{BuildRectFor, ImageProperties, RectBuilder, SugarStack, Text};
use crate::font::fonts::{SugarloafFont, SugarloafFonts};
#[cfg(not(target_arch = "wasm32"))]
use crate::font::loader::Database;
use crate::font::Font;
use crate::font::{
    FONT_ID_EMOJIS, FONT_ID_ICONS, FONT_ID_REGULAR, FONT_ID_SYMBOL, FONT_ID_UNICODE,
};
use crate::glyph::{FontId, GlyphCruncher};
use crate::graphics::SugarloafGraphics;
use crate::layout::SugarloafLayout;
use ab_glyph::{self, Font as GFont, FontArc, Point, PxScale};
use core::fmt::{Debug, Formatter};
use fnv::FnvHashMap;
use unicode_width::UnicodeWidthChar;

#[derive(Debug)]
struct GraphicRect {
    id: crate::graphics::SugarGraphicId,
    #[allow(unused)]
    height: u16,
    width: u16,
    pos: Point,
    columns: f32,
    start_row: f32,
    end_row: f32,
}

#[cfg(target_arch = "wasm32")]
pub struct Database;

/// A little helper struct which contains some additional information about a sugar.
#[derive(Copy, Clone, PartialEq)]
pub struct TextInfo {
    pub font_id: FontId,
    pub px_scale: PxScale,
}

pub struct Sugarloaf {
    sugar_cache: FnvHashMap<char, TextInfo>,
    pub ctx: Context,
    pub layout: SugarloafLayout,
    pub graphics: SugarloafGraphics,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    layer_brush: LayerBrush,
    graphic_rects: FnvHashMap<crate::SugarGraphicId, GraphicRect>,
    rects: Vec<Rect>,
    text_y: f32,
    current_row: u16,
    fonts: SugarloafFonts,
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

pub struct SugarloafWindowSize {
    pub width: u32,
    pub height: u32,
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

unsafe impl raw_window_handle::HasRawWindowHandle for SugarloafWindow {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        self.handle
    }
}

unsafe impl raw_window_handle::HasRawDisplayHandle for SugarloafWindow {
    fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        self.display
    }
}

impl Sugarloaf {
    pub async fn new(
        raw_window_handle: &SugarloafWindow,
        renderer: SugarloafRenderer,
        fonts: SugarloafFonts,
        layout: SugarloafLayout,
        #[allow(unused)] db: Option<&Database>,
    ) -> Result<Sugarloaf, SugarloafWithErrors> {
        let ctx = Context::new(raw_window_handle, &renderer).await;
        let mut sugarloaf_errors = None;

        #[cfg(not(target_arch = "wasm32"))]
        let loader = Font::load(fonts.to_owned(), db);
        #[cfg(target_arch = "wasm32")]
        let loader = Font::load(fonts.to_owned());

        let (loaded_fonts, fonts_not_found) = loader;

        if !fonts_not_found.is_empty() {
            sugarloaf_errors = Some(SugarloafErrors { fonts_not_found });
        }

        let text_brush = text::GlyphBrushBuilder::using_fonts(loaded_fonts)
            .build(&ctx.device, ctx.format);
        let rect_brush = RectBrush::init(&ctx);
        let layer_brush = LayerBrush::new(&ctx);

        let instance = Sugarloaf {
            sugar_cache: FnvHashMap::default(),
            graphics: SugarloafGraphics::new(),
            layer_brush,
            fonts,
            ctx,
            rect_brush,
            rects: vec![],
            graphic_rects: FnvHashMap::default(),
            text_brush,
            text_y: layout.style.screen_position.1,
            current_row: 0,
            layout,
        };

        if let Some(errors) = sugarloaf_errors {
            return Err(SugarloafWithErrors { instance, errors });
        }

        Ok(instance)
    }

    #[allow(unused)]
    pub fn clear(&mut self) {
        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    label: Some("sugarloaf::init -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
    }

    #[inline]
    pub fn update_font(
        &mut self,
        fonts: SugarloafFonts,
        #[allow(unused)] db: Option<&Database>,
    ) -> Option<SugarloafErrors> {
        if self.fonts != fonts {
            log::info!("requested a font change");

            #[cfg(not(target_arch = "wasm32"))]
            let loader = Font::load(fonts.to_owned(), db);
            #[cfg(target_arch = "wasm32")]
            let loader = Font::load(fonts.to_owned());

            let (loaded_fonts, fonts_not_found) = loader;
            if !fonts_not_found.is_empty() {
                return Some(SugarloafErrors { fonts_not_found });
            }

            // Clean font cache per instance
            self.sugar_cache = FnvHashMap::default();

            let text_brush = text::GlyphBrushBuilder::using_fonts(loaded_fonts)
                .build(&self.ctx.device, self.ctx.format);
            self.text_brush = text_brush;
            self.fonts = fonts;
        }

        None
    }

    pub fn get_text_info(&mut self, text: &Text) -> TextInfo {
        *self.sugar_cache.entry(text.content).or_insert_with(|| {
            let font_id = {
                let fonts: &[FontArc] = self.text_brush.fonts();

                fonts
                    .iter()
                    .enumerate()
                    .find(|(_, font_arc)| {
                        font_arc.glyph_id(text.content) != ab_glyph::GlyphId(0)
                    })
                    .map(|(idx, _)| FontId(idx))
                    .unwrap_or(FontId(FONT_ID_REGULAR))
            };

            let char_width = text.content.width().unwrap_or(1);
            let px_scale = match font_id {
                // Icons will look for width 1
                FontId(FONT_ID_ICONS) => PxScale {
                    x: self.layout.scaled_sugarwidth,
                    y: self.layout.scaled_sugarheight,
                },

                FontId(FONT_ID_UNICODE) | FontId(FONT_ID_SYMBOL) => PxScale {
                    x: self.layout.scaled_sugarwidth * char_width as f32,
                    y: self.layout.scaled_sugarheight,
                },

                FontId(FONT_ID_EMOJIS) => PxScale {
                    x: self.layout.scaled_sugarwidth * 2.0,
                    y: self.layout.scaled_sugarheight,
                },

                // FontId(FONT_ID_REGULAR) => {
                // px_scale = Some(PxScale {
                //     x: self.layout.scaled_sugarwidth * 2.0,
                //     y: self.layout.scaled_sugarheight,
                // })
                // }
                FontId(_) => PxScale::from(self.layout.scaled_sugarheight),
            };

            TextInfo { font_id, px_scale }
        })
    }

    #[inline]
    pub fn stack(&mut self, stack: SugarStack) {
        let mut next_text_pos = Point {
            x: self.layout.style.screen_position.0,
            y: self.layout.scaled_sugarheight
                + self.text_y
                + self.layout.style.screen_position.1,
        };

        let mut iterator = stack.into_iter().peekable();
        while iterator.peek().is_some() {
            let text = Text::build_from(&mut iterator, &next_text_pos);
            next_text_pos.x += text.width() as f32 * self.layout.scaled_sugarwidth;

            {
                let owned_text = {
                    let text_info = self.get_text_info(&text);
                    crate::components::text::OwnedText::from((&text, text_info))
                };

                let section = crate::components::text::OwnedSection {
                    screen_position: (text.pos.x, text.pos.y),
                    bounds: (self.layout.width, self.layout.height),
                    text: vec![owned_text],
                    layout: crate::glyph::Layout::default_single_line()
                        .v_align(crate::glyph::VerticalAlign::Bottom)
                        .h_align(crate::glyph::HorizontalAlign::Left),
                };

                self.text_brush.queue(&section);
            }

            {
                let rect_builder = RectBuilder {
                    sugarwidth: self.layout.sugarwidth * 2., // "* 2." because we want some space to the left and right which sums up to one sugarwidth
                    sugarheight: self.layout.sugarheight,
                    line_height: self.layout.line_height,
                    scale: self.ctx.scale,
                };

                self.rects.extend(rect_builder.build_for(&text));
            }

            if let Some(media) = &text.media {
                self.graphic_rects
                    .entry(media.id)
                    .and_modify(|rect| {
                        rect.columns += 1.0;
                        rect.end_row = self.current_row.into();
                    })
                    .or_insert_with(|| {
                        let pos = Point {
                            x: text.pos.x / self.ctx.scale,
                            y: text.pos.y / self.ctx.scale,
                        };

                        GraphicRect {
                            id: media.id,
                            height: media.height,
                            width: media.width,
                            pos,
                            columns: 1.0,
                            start_row: 1.0,
                            end_row: 1.0,
                        }
                    });
            }
        }

        self.current_row += 1;
        self.text_y += self.layout.scaled_sugarheight * self.layout.line_height;
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
    pub fn get_font_bounds(
        &mut self,
        content: char,
        font_id: FontId,
        scale: f32,
    ) -> (f32, f32) {
        let text = crate::components::text::Text {
            text: &content.to_owned().to_string(),
            scale: PxScale { x: scale, y: scale },
            font_id,
            extra: crate::components::text::Extra {
                color: [0., 0., 0., 0.],
                z: 0.0,
            },
        };

        let section = &crate::components::text::Section {
            screen_position: (0., 0.),
            bounds: (self.layout.width, self.layout.height),
            text: vec![text],
            layout: crate::glyph::Layout::default_single_line()
                .v_align(crate::glyph::VerticalAlign::Bottom)
                .h_align(crate::glyph::HorizontalAlign::Left),
        };

        self.text_brush.queue(section);

        if let Some(rect) = self.text_brush.glyph_bounds(section) {
            let width = rect.max.x - rect.min.x;
            let height = rect.max.y - rect.min.y;
            return (width, height);
        }

        (0., 0.)
    }

    #[inline]
    pub fn set_background_color(&mut self, color: wgpu::Color) -> &mut Self {
        self.layout.background_color = color;
        self
    }

    #[inline]
    pub fn set_background_image(&mut self, image: &ImageProperties) -> &mut Self {
        let handle = Handle::from_path(image.path.to_owned());
        self.layout.background_image = Some(layer::types::Image::Raster {
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

    /// calculate_bounds is a fake render operation that defines font bounds
    /// is an important function to figure out the cursor dimensions and background color
    /// but should be used as minimal as possible.
    ///
    /// For example: It is used in Rio terminal only in the initialization and
    /// configuration updates that leads to layout recalculation.
    ///
    #[inline]
    pub fn calculate_bounds(&mut self) {
        // Every time a font size change the cached bounds also changes
        self.sugar_cache = FnvHashMap::default();

        let text_scale = self.layout.style.text_scale;
        // Bounds are defined in runtime
        let font_bound = self.get_font_bounds(' ', FontId(FONT_ID_REGULAR), text_scale);

        self.layout.scaled_sugarwidth = font_bound.0;
        self.layout.scaled_sugarheight = font_bound.1;

        self.layout.sugarwidth = self.layout.scaled_sugarwidth / self.ctx.scale;
        self.layout.sugarheight = self.layout.scaled_sugarheight / self.ctx.scale;

        self.layout.update_columns_per_font_width();
    }

    #[inline]
    fn reset_state(&mut self) {
        self.text_y = self.layout.style.screen_position.1;
        self.rects.clear();
        self.graphic_rects.clear();
        self.current_row = 0;
    }

    #[inline]
    pub fn pile_rects(&mut self, mut instances: Vec<Rect>) -> &mut Self {
        self.rects.append(&mut instances);
        self
    }

    #[inline]
    pub fn text(
        &mut self,
        pos: (f32, f32),
        text_str: String,
        font_id_usize: usize,
        scale: f32,
        color: [f32; 4],
        single_line: bool,
    ) -> &mut Self {
        let font_id = FontId(font_id_usize);

        let text = crate::components::text::Text {
            text: &text_str,
            scale: PxScale::from(scale * self.ctx.scale),
            font_id,
            extra: crate::components::text::Extra { color, z: 0.0 },
        };

        let layout = if single_line {
            crate::glyph::Layout::default_single_line()
                .v_align(crate::glyph::VerticalAlign::Bottom)
                .h_align(crate::glyph::HorizontalAlign::Left)
        } else {
            crate::glyph::Layout::default()
                .v_align(crate::glyph::VerticalAlign::Bottom)
                .h_align(crate::glyph::HorizontalAlign::Left)
        };

        let section = &crate::components::text::Section {
            screen_position: (pos.0 * self.ctx.scale, pos.1 * self.ctx.scale),
            bounds: (self.layout.width, self.layout.height),
            text: vec![text],
            layout,
        };

        self.text_brush.queue(section);
        self
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.ctx.resize(width, height);
        self.layout.resize(width, height).update();
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) {
        self.ctx.scale = scale;
        self.layout.rescale(scale).update();
    }

    #[inline]
    pub fn render(&mut self) {
        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    label: Some("sugarloaf::render -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                if let Some(bg_image) = &self.layout.background_image {
                    self.layer_brush.prepare_ref(
                        &mut encoder,
                        &mut self.ctx,
                        &[bg_image],
                    );

                    self.layer_brush
                        .render_with_encoder(0, view, &mut encoder, None);
                }

                self.rect_brush.render(
                    &mut encoder,
                    view,
                    (self.ctx.size.width, self.ctx.size.height),
                    &self.rects,
                    &mut self.ctx,
                );

                self.rects = vec![];
                self.current_row = 0;

                let _ = self
                    .text_brush
                    .draw_queued(&mut self.ctx, &mut encoder, view);

                self.layer_brush.end_frame();

                for entry_render in
                    &self.graphic_rects.keys().cloned().collect::<Vec<_>>()
                {
                    if let Some(entry) = self.graphic_rects.get(entry_render) {
                        if let Some(graphic_data) = self.graphics.get(&entry.id) {
                            let rows = entry.end_row - entry.start_row;
                            println!("{:?}", entry.columns);
                            println!("{:?}", rows);
                            println!("{:?}", self.current_row);
                            println!("{:?}", (rows) * self.layout.scaled_sugarheight);

                            let height = (rows - 2.) * self.layout.scaled_sugarheight;

                            let a = layer::types::Image::Raster {
                                handle: graphic_data.handle.clone(),
                                bounds: Rectangle {
                                    x: entry.pos.x,
                                    y: entry.pos.y,
                                    width: entry.width as f32,
                                    height,
                                },
                            };

                            self.layer_brush.prepare_ref(
                                &mut encoder,
                                &mut self.ctx,
                                &[&a],
                            );

                            self.layer_brush.render_with_encoder(
                                0,
                                view,
                                &mut encoder,
                                None,
                            );
                        }
                    }
                }
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                panic!(
                    "Swapchain error: {}. Rendering cannot continue.",
                    wgpu::SurfaceError::OutOfMemory
                )
            }
            Err(_) => {}
        }

        self.reset_state();
    }
}
