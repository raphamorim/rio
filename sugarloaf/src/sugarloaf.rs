use crate::components::rect::{Rect, RectBrush};
use crate::components::text;
use crate::context::Context;
use crate::core::SugarStack;
use crate::font::Font;
use crate::layout::SugarloafLayout;
use glyph_brush::ab_glyph::{self, Font as GFont, FontArc};
use glyph_brush::{FontId, GlyphCruncher, OwnedSection, OwnedText};

pub fn orthographic_projection(width: u32, height: u32) -> [f32; 16] {
    [
        2.0 / width as f32,
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / height as f32,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -1.0,
        1.0,
        0.0,
        1.0,
    ]
}

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
        transform: [f32; 16],
        instances: &[Rect],
        context: &mut Context,
    );
}

type FontBound = (f32, f32);

#[derive(Default)]
struct FontBounds {
    default: FontBound,
    symbols: FontBound,
    emojis: FontBound,
    unicode: FontBound,
}

pub struct Sugarloaf {
    pub ctx: Context,
    pub layout: SugarloafLayout,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    rects: Vec<Rect>,
    acc_line: f32,
    acc_line_y: f32,
    font_bounds: FontBounds,
    font_name: String,
}

const FONT_ID_REGULAR: usize = 0;
const FONT_ID_SYMBOL: usize = 1;
const FONT_ID_EMOJIS: usize = 2;
const FONT_ID_UNICODE: usize = 3;
const FONT_ID_BOLD: usize = 4;
const FONT_ID_ITALIC: usize = 5;
const FONT_ID_BOLD_ITALIC: usize = 6;

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
            font.symbol,
            font.emojis,
            font.unicode,
            font.text.bold,
            font.text.italic,
            font.text.bold_italic,
        ])
        .build(&ctx.device, ctx.format);
        let rect_brush = RectBrush::init(&ctx);
        Ok(Sugarloaf {
            font_name,
            ctx,
            rect_brush,
            rects: vec![],
            text_brush,
            acc_line: 0.0,
            acc_line_y: 0.0,
            font_bounds: FontBounds::default(),
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

    pub fn update_font(&mut self, font_name: String) -> &mut Self {
        if self.font_name != font_name {
            log::info!("requested a font change {font_name}");
            let font = Font::new(font_name.to_string());

            let text_brush = text::GlyphBrushBuilder::using_fonts(vec![
                font.text.regular,
                font.symbol,
                font.emojis,
                font.unicode,
                font.text.bold,
                font.text.italic,
                font.text.bold_italic,
            ])
            .build(&self.ctx.device, self.ctx.format);
            self.text_brush = text_brush;
            self.font_name = font_name;
        }
        self
    }

    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.ctx.resize(width, height);
        self.layout.resize(width, height).update();
        self
    }

    pub fn rescale(&mut self, scale: f32) -> &mut Self {
        self.ctx.scale = scale;
        self.layout.rescale(scale).update();
        self
    }

    #[inline]
    pub fn stack(&mut self, stack: SugarStack) {
        let mut text: Vec<OwnedText> = vec![];
        let mut x = 0.;
        let mut mod_size = 2.0;
        mod_size /= self.ctx.scale;

        let fonts = self.text_brush.fonts();
        let regular: &FontArc = &fonts[0];
        let symbols: &FontArc = &fonts[1];
        let emojis: &FontArc = &fonts[2];
        let unicode: &FontArc = &fonts[3];
        let glyph_zero = ab_glyph::GlyphId(0);

        for sugar in stack.iter() {
            let mut add_pos_x = self.font_bounds.default.0;

            let mut font_id: FontId = if regular.glyph_id(sugar.content) != glyph_zero {
                FontId(FONT_ID_REGULAR)
            } else if symbols.glyph_id(sugar.content) != glyph_zero {
                add_pos_x = self.font_bounds.symbols.0;
                FontId(FONT_ID_SYMBOL)
            } else if emojis.glyph_id(sugar.content) != glyph_zero {
                add_pos_x = self.font_bounds.emojis.0;
                FontId(FONT_ID_EMOJIS)
            } else if unicode.glyph_id(sugar.content) != glyph_zero {
                add_pos_x = self.font_bounds.unicode.0;
                FontId(FONT_ID_UNICODE)
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

            if self.acc_line_y == 0.0 {
                self.acc_line_y = (self.layout.style.screen_position.1
                    - self.font_bounds.default.1)
                    / self.ctx.scale;
            }

            text.push(
                OwnedText::new(sugar.content.to_owned())
                    .with_font_id(font_id)
                    .with_color(sugar.foreground_color)
                    .with_scale(self.layout.style.text_scale),
            );

            self.rects.push(Rect {
                position: [
                    (self.layout.style.screen_position.0 / self.ctx.scale) + x,
                    self.acc_line_y,
                ],
                color: sugar.background_color,
                size: [add_pos_x * mod_size, ((self.font_bounds.default.0 * mod_size).ceil() + mod_size)],
            });

            if let Some(decoration) = &sugar.decoration {
                let dx = add_pos_x;
                let dy = self.font_bounds.default.0;
                self.rects.push(Rect {
                    position: [
                        (self.layout.style.screen_position.0 / self.ctx.scale)
                            + x
                            + ((dx * decoration.position.0) / self.ctx.scale),
                        self.acc_line_y + dy * (decoration.position.1 * mod_size),
                    ],
                    color: decoration.color,
                    size: [
                        (dx * decoration.size.0) * mod_size,
                        ((dy * decoration.size.1) * mod_size).ceil() + mod_size,
                    ],
                });
            }

            x += add_pos_x / self.ctx.scale;
        }

        let section = &OwnedSection {
            screen_position: (
                self.layout.style.screen_position.0,
                self.layout.style.screen_position.1 + self.acc_line,
            ),
            bounds: self.layout.style.bounds,
            text,
            layout: glyph_brush::Layout::default_single_line()
                .v_align(glyph_brush::VerticalAlign::Bottom)
                .h_align(glyph_brush::HorizontalAlign::Left),
        };

        self.text_brush.queue(section);

        self.acc_line_y =
            (self.layout.style.screen_position.1 + self.acc_line) / self.ctx.scale;
        self.acc_line += self.font_bounds.default.1;
    }

    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    pub fn get_scale(&self) -> f32 {
        self.ctx.scale
    }

    #[inline]
    pub fn get_font_bounds(&mut self, content: char, font_id: FontId) -> FontBound {
        let text = vec![OwnedText::new(content)
            .with_font_id(font_id)
            .with_color([0., 0., 0., 0.])
            .with_scale(self.layout.style.text_scale)];

        let section = &OwnedSection {
            screen_position: (
                self.layout.style.screen_position.0,
                self.layout.style.screen_position.1 + self.acc_line,
            ),
            bounds: self.layout.style.bounds,
            text,
            layout: glyph_brush::Layout::default_single_line()
                .v_align(glyph_brush::VerticalAlign::Bottom)
                .h_align(glyph_brush::HorizontalAlign::Left),
        };

        self.text_brush.queue(section);

        if let Some(rect) = self.text_brush.glyph_bounds(section) {
            let width = rect.max.x - rect.min.x;
            let height = rect.max.y - rect.min.y;
            return (width, height);
        }

        (0., 0.)
    }

    /// config is a fake render operation that defines font bounds
    /// is an important function to figure out the cursor dimensions and background color
    /// but should be used as minimal as possible.
    ///
    /// For example: It is used in Rio terminal only in the initialization and
    /// configuration updates that leads to layout recalculation.
    ///
    #[inline]
    pub fn config(&mut self, color: wgpu::Color) {
        self.reset_state();
        self.rects = vec![];
        self.layout.background_color = color;

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
                            load: wgpu::LoadOp::Clear(color),
                            store: true,
                        },
                    })],
                    depth_stencil_attachment: None,
                });

                // Bounds are defined in runtime
                self.font_bounds.default = self.get_font_bounds(' ', FontId(0));

                self.layout.update_columns_lines_per_font_bound(self.font_bounds.default.0);

                self.font_bounds.symbols =
                    // U+2AF9 => \u{2AF9} => ⫹
                    self.get_font_bounds('\u{2AF9}', FontId(1));
                self.font_bounds.emojis =
                    // U+1F947 => \u{1F947} => 🥇
                    self.get_font_bounds('\u{1F947}', FontId(2));
                self.font_bounds.unicode =
                    // U+33D1 => \u{33D1} => ㏑
                    self.get_font_bounds('\u{33D1}', FontId(3));

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

    fn reset_state(&mut self) {
        self.acc_line = 0.0;
        self.acc_line_y = 0.0;
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
                    orthographic_projection(self.ctx.size.width, self.ctx.size.height),
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
