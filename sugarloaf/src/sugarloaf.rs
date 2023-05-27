use crate::components::rect::{Rect, RectBrush};
use crate::components::text;
use crate::context::Context;
use crate::core::{SugarStack, SugarloafStyle};
use crate::font::Font;
use glyph_brush::ab_glyph::{self, Font as GFont, FontArc};
use glyph_brush::{FontId, GlyphCruncher, OwnedSection, OwnedText, Section, Text};
#[cfg(target_arch = "wasm32")]
use web_sys::{ImageBitmapRenderingContext, OffscreenCanvas};

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
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    rects: Vec<Rect>,
    acc_line: f32,
    acc_line_y: f32,
    initial_scale: f32,
    font_bounds: FontBounds,
    background_color: wgpu::Color,
}

impl Sugarloaf {
    pub async fn new(
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
        font_name: String,
    ) -> Result<Sugarloaf, String> {
        let ctx = Context::new(winit_window, power_preference).await;

        match Font::new(font_name) {
            Ok(font) => {
                let text_brush = text::GlyphBrushBuilder::using_fonts(vec![
                    font.system,
                    font.symbol,
                    font.emojis,
                    font.unicode,
                ])
                .build(&ctx.device, ctx.format);
                let rect_brush = RectBrush::init(&ctx);
                Ok(Sugarloaf {
                    initial_scale: ctx.scale,
                    ctx,
                    rect_brush,
                    rects: vec![],
                    text_brush,
                    acc_line: 0.0,
                    acc_line_y: 0.0,
                    font_bounds: FontBounds::default(),
                    background_color: wgpu::Color::BLACK,
                })
            }
            Err(err_message) => Err(format!(
                "Renderer error: could not initialize font {err_message:?}"
            )),
        }
    }

    pub fn tabs(
        &mut self,
        _text: String,
        style: SugarloafStyle,
        color_inactive: [f32; 4],
        color_active: [f32; 4],
    ) {
        self.text_brush.queue(Section {
            screen_position: style.screen_position,
            bounds: style.bounds,
            text: vec![
                Text::new("■")
                    .with_color(color_active)
                    .with_scale(style.text_scale),
                Text::new("and more 2 tabs")
                    .with_color(color_inactive)
                    .with_scale(style.text_scale),
                // Text::new(&fps_text)
                //     .with_color(self.config.colors.foreground)
                //     .with_scale(self.styles.tabs_active.text_scale),
            ],
            layout: glyph_brush::Layout::default_single_line(),
            // ..Section::default() // .line_breaker(glyph_brush::BuiltInLineBreaker::UNi)
            // .v_align(glyph_brush::VerticalAlign::Center)
            // .h_align(glyph_brush::HorizontalAlign::Left)
        });

        // self.brush.queue(Section {
        //     screen_position: ((self.size.width as f32 - 20.0) * scale, (8.0 * scale)),
        //     bounds: (
        //         (self.size.width as f32) - (40.0 * scale),
        //         (self.size.height as f32) * scale,
        //     ),
        //     text: vec![Text::new("■ vim ■ zsh ■ docker")
        //         //(157,165,237)
        //         .with_color([0.89020, 0.54118, 0.33725, 1.0])
        //         .with_scale(14.0 * scale)],
        //     layout: glyph_brush::Layout::default()
        //         // .line_breaker(glyph_brush::BuiltInLineBreaker::UNi)
        //         // .v_align(glyph_brush::VerticalAlign::Center)
        //         .h_align(glyph_brush::HorizontalAlign::Right),
        //     ..Section::default()
        // });
        // }
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
                            load: wgpu::LoadOp::Clear(self.background_color),
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

    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.ctx.resize(width, height);
        self
    }

    pub fn rescale(&mut self, scale: f32) -> &mut Self {
        self.ctx.scale = scale;
        self
    }

    #[inline]
    pub fn stack(&mut self, stack: SugarStack, style: SugarloafStyle) {
        let mut text: Vec<OwnedText> = vec![];
        let mut x = 0.;
        let mut mod_size = 1.0;

        if self.acc_line_y == 0.0 {
            self.acc_line_y =
                (style.screen_position.1 - style.text_scale) / self.ctx.scale;
        }

        // TODO: Rewrite this method to proper use scale and get rid of initial_scale
        // Is not the most optimal way record original scale and this has been happening due
        // to not updating correctly scale values
        if self.initial_scale < 2.0 {
            mod_size += self.initial_scale;
        }

        let fonts = self.text_brush.fonts();
        let system: &FontArc = &fonts[0];
        let symbols: &FontArc = &fonts[1];
        let emojis: &FontArc = &fonts[2];
        let unicode: &FontArc = &fonts[3];
        let glyph_zero = ab_glyph::GlyphId(0);

        for sugar in stack.iter() {
            let mut add_pos_x = self.font_bounds.default.0;

            let font_id: FontId = if system.glyph_id(sugar.content) != glyph_zero {
                FontId(0)
            } else if symbols.glyph_id(sugar.content) != glyph_zero {
                add_pos_x = self.font_bounds.symbols.0;
                FontId(1)
            } else if emojis.glyph_id(sugar.content) != glyph_zero {
                add_pos_x = self.font_bounds.emojis.0;
                FontId(2)
            } else if unicode.glyph_id(sugar.content) != glyph_zero {
                add_pos_x = self.font_bounds.unicode.0;
                FontId(3)
            } else {
                FontId(0)
            };

            text.push(
                OwnedText::new(sugar.content.to_owned())
                    .with_font_id(font_id)
                    .with_color(sugar.foreground_color)
                    .with_scale(style.text_scale),
            );

            self.rects.push(Rect {
                position: [
                    (style.screen_position.0 / self.ctx.scale) + x,
                    self.acc_line_y,
                ],
                color: sugar.background_color,
                size: [add_pos_x * mod_size, self.font_bounds.default.0 * mod_size],
            });

            x += add_pos_x / self.initial_scale;
        }

        let section = &OwnedSection {
            screen_position: (
                style.screen_position.0,
                style.screen_position.1 + self.acc_line,
            ),
            bounds: style.bounds,
            text,
            layout: glyph_brush::Layout::default_single_line()
                .v_align(glyph_brush::VerticalAlign::Bottom),
        };

        self.text_brush.queue(section);

        self.acc_line_y = (style.screen_position.1 + self.acc_line) / self.ctx.scale;
        self.acc_line += style.text_scale;
    }

    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    pub fn get_scale(&self) -> f32 {
        self.ctx.scale
    }

    #[inline]
    pub fn get_font_bounds(
        &mut self,
        content: char,
        font_id: FontId,
        style: SugarloafStyle,
    ) -> FontBound {
        let text = vec![OwnedText::new(content)
            .with_font_id(font_id)
            .with_color([0., 0., 0., 0.])
            .with_scale(style.text_scale)];

        let section = &OwnedSection {
            screen_position: (
                style.screen_position.0,
                style.screen_position.1 + self.acc_line,
            ),
            bounds: style.bounds,
            text,
            layout: glyph_brush::Layout::default_single_line()
                .v_align(glyph_brush::VerticalAlign::Bottom),
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
    pub fn init(&mut self, color: wgpu::Color, style: SugarloafStyle) {
        self.reset_state();
        self.rects = vec![];
        self.background_color = color;

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

                if self.font_bounds.default == (0., 0.) {
                    // Bounds are defined in runtime
                    self.font_bounds.default =
                        self.get_font_bounds(' ', FontId(0), style);
                    self.font_bounds.symbols =
                        // U+2AF9 => \u{2AF9} => ⫹
                        self.get_font_bounds('\u{2AF9}', FontId(1), style);
                    self.font_bounds.emojis =
                        // U+1F947 => \u{1F947} => 🥇
                        self.get_font_bounds('\u{1F947}', FontId(2), style);
                    self.font_bounds.unicode =
                        // U+33D1 => \u{33D1} => ㏑
                        self.get_font_bounds('\u{33D1}', FontId(3), style);
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
    }

    fn reset_state(&mut self) {
        self.acc_line = 0.0;
        self.acc_line_y = 0.0;
    }

    pub fn pile_rect(&mut self, instances: Vec<Rect>) -> &mut Self {
        self.rects = instances;
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
                            load: wgpu::LoadOp::Clear(self.background_color),
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
