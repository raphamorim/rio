use crate::components::rect::{Rect, RectBrush};
use crate::components::text;
use crate::context::Context;
use crate::core::Sugar;
use crate::core::SugarStack;
use crate::font::Font;
use crate::layout::SugarloafLayout;
use glyph_brush::ab_glyph::{self, Font as GFont, FontArc, PxScale};
use glyph_brush::{FontId, GlyphCruncher};
use std::collections::HashMap;
use unicode_width::UnicodeWidthChar;

pub trait Renderable: 'static + Sized {
    fn required_features() -> wgpu::Features {
        wgpu::Features::empty()
    }
    fn required_downlevel_capabilities() -> wgpu::DownlevelCapabilities {
        wgpu::DownlevelCapabilities {
            flags: wgpu::DownlevelFlags::empty(),
            shader_model: wgpu::ShaderModel::Sm5,
            ..wgpu::DownlevelCapabilities::default()
        }
    }
    fn required_limits() -> wgpu::Limits {
        // These downlevel limits will allow the code to run on all possible hardware
        wgpu::Limits::downlevel_webgl2_defaults()
    }
    fn init(context: &Context) -> Self;
    fn resize(
        &mut self,
        config: &wgpu::SurfaceConfiguration,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    );
    fn update(&mut self, event: winit::event::WindowEvent);
    fn render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        dimensions: (u32, u32),
        instances: &[Rect],
        context: &mut Context,
    );
}

#[derive(Copy, Clone, PartialEq)]
pub struct CachedSugar {
    font_id: FontId,
    char_width: f32,
}

pub struct Sugarloaf {
    sugar_cache: HashMap<char, CachedSugar>,
    pub ctx: Context,
    pub layout: SugarloafLayout,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    rects: Vec<Rect>,
    text_y: f32,
    font_bound: (f32, f32),
    font_name: String,
}

const FONT_ID_REGULAR: usize = 0;
const FONT_ID_ITALIC: usize = 1;
const FONT_ID_BOLD: usize = 2;
const FONT_ID_BOLD_ITALIC: usize = 3;
// TODO: Implement variants
#[allow(dead_code)]
const FONT_ID_EXTRA_LIGHT: usize = 4;
#[allow(dead_code)]
const FONT_ID_EXTRA_LIGHT_ITALIC: usize = 5;
#[allow(dead_code)]
const FONT_ID_LIGHT: usize = 6;
#[allow(dead_code)]
const FONT_ID_LIGHT_ITALIC: usize = 7;
#[allow(dead_code)]
const FONT_ID_SEMI_BOLD: usize = 8;
#[allow(dead_code)]
const FONT_ID_SEMI_BOLD_ITALIC: usize = 9;
#[allow(dead_code)]
const FONT_ID_SEMI_LIGHT: usize = 10;
#[allow(dead_code)]
const FONT_ID_SEMI_LIGHT_ITALIC: usize = 11;
const FONT_ID_SYMBOL: usize = 12;
const FONT_ID_EMOJIS: usize = 13;
const FONT_ID_UNICODE: usize = 14;
const FONT_ID_ICONS: usize = 15;

impl Sugarloaf {
    pub async fn new(
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
        font_name: String,
        layout: SugarloafLayout,
    ) -> Result<Sugarloaf, String> {
        let ctx = Context::new(winit_window, power_preference).await;

        let font = Font::new(font_name.to_string());

        let text_brush = text::GlyphBrushBuilder::using_fonts(vec![
            font.text.regular,
            font.text.italic,
            font.text.bold,
            font.text.bold_italic,
            font.text.extra_light,
            font.text.extra_light_italic,
            font.text.light,
            font.text.light_italic,
            font.text.semi_bold,
            font.text.semi_bold_italic,
            font.text.semi_light,
            font.text.semi_light_italic,
            font.symbol,
            font.emojis,
            font.unicode,
            font.icons,
        ])
        .build(&ctx.device, ctx.format);
        let rect_brush = RectBrush::init(&ctx);
        Ok(Sugarloaf {
            sugar_cache: HashMap::new(),
            font_name,
            ctx,
            rect_brush,
            rects: vec![],
            text_brush,
            text_y: 0.0,
            font_bound: (0.0, 0.0),
            layout,
        })
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
                    label: Some("sugarloaf::init -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });
                self.ctx.staging_belt.finish();
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
                self.ctx.staging_belt.recall();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
    }

    #[inline]
    pub fn update_font(&mut self, font_name: String) -> &mut Self {
        if self.font_name != font_name {
            log::info!("requested a font change {font_name}");
            let font = Font::new(font_name.to_string());

            // Clean font cache per instance
            self.sugar_cache = HashMap::new();

            let text_brush = text::GlyphBrushBuilder::using_fonts(vec![
                font.text.regular,
                font.text.italic,
                font.text.bold,
                font.text.bold_italic,
                font.text.extra_light,
                font.text.extra_light_italic,
                font.text.light,
                font.text.light_italic,
                font.text.semi_bold,
                font.text.semi_bold_italic,
                font.text.semi_light,
                font.text.semi_light_italic,
                font.symbol,
                font.emojis,
                font.unicode,
                font.icons,
            ])
            .build(&self.ctx.device, self.ctx.format);
            self.text_brush = text_brush;
            self.font_name = font_name;
        }
        self
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.ctx.resize(width, height);
        self.layout.resize(width, height).update();
        self
    }

    #[inline]
    pub fn rescale(&mut self, scale: f32) -> &mut Self {
        self.ctx.scale = scale;
        self.layout.rescale(scale).update();
        self
    }

    #[inline]
    pub fn get_font_id(&mut self, sugar: &mut Sugar) -> CachedSugar {
        if let Some(cached_sugar) = self.sugar_cache.get(&sugar.content) {
            return *cached_sugar;
        }

        #[allow(clippy::unnecessary_to_owned)]
        let fonts: &[FontArc] = &self.text_brush.fonts().to_owned();
        let regular: &FontArc = &fonts[FONT_ID_REGULAR];
        let symbols: &FontArc = &fonts[FONT_ID_SYMBOL];
        let emojis: &FontArc = &fonts[FONT_ID_EMOJIS];
        let unicode: &FontArc = &fonts[FONT_ID_UNICODE];
        let icons: &FontArc = &fonts[FONT_ID_ICONS];
        let glyph_zero = ab_glyph::GlyphId(0);

        let mut font_id: FontId = if regular.glyph_id(sugar.content) != glyph_zero {
            FontId(FONT_ID_REGULAR)
        } else if symbols.glyph_id(sugar.content) != glyph_zero {
            FontId(FONT_ID_SYMBOL)
        } else if emojis.glyph_id(sugar.content) != glyph_zero {
            FontId(FONT_ID_EMOJIS)
        } else if unicode.glyph_id(sugar.content) != glyph_zero {
            FontId(FONT_ID_UNICODE)
        } else if icons.glyph_id(sugar.content) != glyph_zero {
            FontId(FONT_ID_ICONS)
        } else {
            FontId(FONT_ID_REGULAR)
        };

        if font_id == FontId(FONT_ID_REGULAR) {
            if let Some(style) = &sugar.style {
                if style.is_bold_italic {
                    font_id = FontId(FONT_ID_BOLD_ITALIC);
                } else if style.is_bold {
                    font_id = FontId(FONT_ID_BOLD);
                } else if style.is_italic {
                    font_id = FontId(FONT_ID_ITALIC);
                }
            }
        }

        let char_width = sugar.content.width().unwrap_or(1) as f32;
        let cached_sugar = CachedSugar {
            font_id,
            char_width,
        };

        self.sugar_cache.insert(
            sugar.content,
            CachedSugar {
                font_id,
                char_width,
            },
        );

        cached_sugar
    }

    #[inline]
    pub fn stack(&mut self, mut stack: SugarStack) {
        let mut x = 0.;
        let mod_pos_y = self.layout.style.screen_position.1;
        let mod_text_y = self.layout.sugarheight * self.ctx.scale / 2.;

        let sugar_x = self.layout.sugarwidth * self.ctx.scale;
        let sugar_width = self.layout.sugarwidth * 2.;
        for sugar in stack.iter_mut() {
            let mut add_pos_x = sugar_x;
            let mut sugar_char_width = 1.;
            let cached_sugar: CachedSugar = self.get_font_id(sugar);

            if cached_sugar.char_width > 1. {
                sugar_char_width += 1.;
                add_pos_x += sugar_x;
            }

            if self.text_y == 0.0 {
                self.text_y = self.layout.style.screen_position.1;
            }

            let mut scale = self.layout.style.text_scale;
            if cached_sugar.font_id == FontId(FONT_ID_ICONS) {
                scale /= 2.0;
            }

            let text = crate::components::text::Text {
                text: &sugar.content.to_owned().to_string(),
                scale: PxScale::from(scale),
                font_id: cached_sugar.font_id,
                extra: crate::components::text::Extra {
                    color: sugar.foreground_color,
                    z: 0.0,
                },
            };

            let rect_pos_x = self.layout.style.screen_position.0 + x;
            let rect_pos_y = self.text_y + mod_pos_y;
            let width_bound = sugar_width * sugar_char_width;

            let section = &crate::components::text::Section {
                screen_position: (rect_pos_x, mod_text_y + self.text_y + mod_pos_y),
                bounds: (width_bound, self.layout.sugarheight * self.ctx.scale),
                text: vec![text],
                layout: glyph_brush::Layout::default_single_line()
                    .v_align(glyph_brush::VerticalAlign::Center)
                    .h_align(glyph_brush::HorizontalAlign::Left),
            };

            self.text_brush.queue(section);

            let scaled_rect_pos_x = rect_pos_x / self.ctx.scale;
            let scaled_rect_pos_y = rect_pos_y / self.ctx.scale;
            self.rects.push(Rect {
                position: [scaled_rect_pos_x, scaled_rect_pos_y],
                color: sugar.background_color,
                size: [width_bound, self.layout.sugarheight],
            });

            if let Some(decoration) = &sugar.decoration {
                // TODO:
                //  let dec_position_y = match decoration.position.1 {
                //     SugarDecorationPositionY::Bottom(pos_decoration_y) => {
                //         scaled_rect_pos_y + ((pos_decoration_y) * self.ctx.scale)
                //     }
                //     SugarDecorationPositionY::Top(pos_decoration_y) => {
                //         scaled_rect_pos_y + pos_decoration_y
                //     }
                //     SugarDecorationPositionY::Middle(pos_decoration_y) => {
                //         scaled_rect_pos_y + (self.layout.sugarheight / 2.0) + pos_decoration_y
                //     }
                // };

                let dec_pos_y = (scaled_rect_pos_y)
                    + (decoration.relative_position.1 * self.layout.line_height);
                // A decoration with is_content_positioned has the width and height based on font_size
                // and in this way is not affected by line_height (useful for decorations like Block and Beam)
                // if decoration.is_content_positioned {
                //     self.rects.push(Rect {
                //         position: [
                //             (scaled_rect_pos_x
                //                 + (add_pos_x * decoration.relative_position.0)
                //                     / self.ctx.scale),
                //             scaled_rect_pos_y,
                //         ],
                //         color: decoration.color,
                //         size: [
                //             (width_bound * decoration.size.0),
                //             (self.layout.font_size) + decoration.size.1,
                //         ],
                //     });
                // } else {
                self.rects.push(Rect {
                    position: [
                        (scaled_rect_pos_x
                            + (add_pos_x * decoration.relative_position.0)
                                / self.ctx.scale),
                        dec_pos_y,
                    ],
                    color: decoration.color,
                    size: [
                        (width_bound * decoration.size.0),
                        (self.layout.sugarheight) * decoration.size.1,
                    ],
                });
                // }
            }

            x += add_pos_x;
        }

        self.text_y += self.font_bound.1;
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
    pub fn get_font_bounds(&mut self, content: char, font_id: FontId) -> (f32, f32) {
        let scale = self.layout.style.text_scale;

        let text = crate::components::text::Text {
            text: &content.to_owned().to_string(),
            scale: PxScale::from(scale),
            font_id,
            extra: crate::components::text::Extra {
                color: [0., 0., 0., 0.],
                z: 0.0,
            },
        };

        let section = &crate::components::text::Section {
            screen_position: (0., 0.),
            bounds: (scale, scale),
            text: vec![text],
            layout: glyph_brush::Layout::default_single_line()
                .v_align(glyph_brush::VerticalAlign::Center)
                .h_align(glyph_brush::HorizontalAlign::Left),
        };

        self.text_brush.queue(section);

        if let Some(rect) = self.text_brush.glyph_bounds(section) {
            let width = rect.max.x - rect.min.x;
            let height = rect.max.y - rect.min.y;
            return (width, height * self.layout.line_height);
        }

        (0., 0.)
    }

    #[inline]
    pub fn set_background_color(&mut self, color: wgpu::Color) -> &mut Self {
        self.layout.background_color = color;
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
        self.reset_state();
        self.rects = vec![];

        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sugarloaf::init -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                // Bounds are defined in runtime
                self.font_bound = self.get_font_bounds(' ', FontId(FONT_ID_REGULAR));

                self.layout.sugarwidth = self.font_bound.0;
                self.layout.sugarheight = self.font_bound.1;

                self.layout.sugarwidth /= self.ctx.scale;
                self.layout.sugarheight /= self.ctx.scale;

                self.layout
                    .update_columns_lines_per_font_bound(self.font_bound.0);

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
    fn reset_state(&mut self) {
        self.text_y = 0.0;
    }

    pub fn bytes(&self, width: u32, height: u32) -> Vec<u8> {
        let dst_texture = self.ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("destination"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let dst_buffer = self.ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("image map buffer"),
            size: width as u64 * height as u64 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut cmd_buf = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        cmd_buf.copy_texture_to_buffer(
            wgpu::ImageCopyTexture {
                texture: &dst_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::ImageCopyBuffer {
                buffer: &dst_buffer,
                layout: wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        self.ctx.queue.submit(Some(cmd_buf.finish()));

        let dst_buffer_slice = dst_buffer.slice(..);
        dst_buffer_slice.map_async(wgpu::MapMode::Read, |_| ());
        self.ctx.device.poll(wgpu::Maintain::Wait);
        let bytes = dst_buffer_slice.get_mapped_range().to_vec();
        bytes
    }

    #[inline]
    pub fn pile_rect(&mut self, mut instances: Vec<Rect>) -> &mut Self {
        self.rects.append(&mut instances);
        self
    }

    #[inline]
    pub fn render(&mut self) {
        self.reset_state();

        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("sugarloaf::render -> Clear frame"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(self.layout.background_color),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                self.rect_brush.render(
                    &mut encoder,
                    view,
                    (self.ctx.size.width, self.ctx.size.height),
                    &self.rects,
                    &mut self.ctx,
                );

                self.rects = vec![];

                let _ = self.text_brush.draw_queued(
                    &self.ctx.device,
                    &mut self.ctx.staging_belt,
                    &mut encoder,
                    view,
                    (self.ctx.size.width, self.ctx.size.height),
                );

                self.ctx.staging_belt.finish();
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
                self.ctx.staging_belt.recall();
            }
            Err(error) => {
                if error == wgpu::SurfaceError::OutOfMemory {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
            }
        }
    }
}
