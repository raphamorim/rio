use crate::components::rect::{Rect, RectBrush};
use crate::components::text;
use crate::context::Context;
use crate::core::{SugarStack, SugarloafStyle};
use crate::font::Font;
use glyph_brush::ab_glyph::Font as GFont;
use glyph_brush::ab_glyph::{self, FontArc};
use glyph_brush::FontId;
use glyph_brush::GlyphCruncher;
use glyph_brush::{OwnedSection, OwnedText};

// TODO: Use macro instead
pub enum RendererTarget {
    Desktop,
    Web,
}

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

pub struct Sugarloaf {
    pub ctx: Context,
    text_brush: text::GlyphBrush<()>,
    rect_brush: RectBrush,
    rects: Vec<Rect>,
    acc_line: f32,
    acc_line_y: f32,
    sugar_dimension: (f32, f32),
}

// TODO: Sugarloaf integrate CustomRenderer (or Renderer) layer usage
impl Sugarloaf {
    pub async fn new(
        target: RendererTarget,
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
        font_name: String,
    ) -> Result<Sugarloaf, String> {
        let ctx = match target {
            RendererTarget::Desktop => Context::new(winit_window, power_preference).await,
            RendererTarget::Web => {
                todo!("web not implemented");
            }
        };

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
                    ctx,
                    rect_brush,
                    rects: vec![],
                    text_brush,
                    acc_line: 0.0,
                    acc_line_y: 0.0,
                    sugar_dimension: (0., 0.)
                })
            }
            Err(err_message) => Err(format!(
                "Renderer error: could not initialize font {err_message:?}"
            )),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) -> &mut Self {
        self.ctx.size.width = width;
        self.ctx.size.height = height;
        self.ctx.surface.configure(
            &self.ctx.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.ctx.format,
                width,
                height,
                view_formats: vec![],
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        self
    }

    pub fn rescale(&mut self, scale: f32) -> &mut Self {
        self.ctx.scale = scale;
        self
    }

    pub fn stack(&mut self, stack: SugarStack, style: SugarloafStyle) {
        let mut text: Vec<OwnedText> = vec![];
        let mut x = 0.;

        if self.acc_line_y == 0.0 {
            self.acc_line_y = 20.;
            println!("{:?}", self.acc_line_y);
            // self.acc_line_y = self.acc_line_y;
        }

        let fonts = self.text_brush.fonts();
        let system: &FontArc = &fonts[0];
        let symbols: &FontArc = &fonts[1];
        let emojis: &FontArc = &fonts[2];
        let unicode: &FontArc = &fonts[3];
        let glyph_zero = ab_glyph::GlyphId(0);

        for sugar in stack.iter() {
            let font_id: FontId = if system.glyph_id(sugar.content) != glyph_zero {
                FontId(0)
            } else if symbols.glyph_id(sugar.content) != glyph_zero {
                FontId(1)
            } else if emojis.glyph_id(sugar.content) != glyph_zero {
                FontId(2)
            } else if unicode.glyph_id(sugar.content) != glyph_zero {
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

            // Sugar dimension will return 150px
            // (style.screen_position.1 + self.acc_line) / 2.

            self.rects.push(Rect {
                position: [style.screen_position.0 - 30. + x, self.acc_line_y],
                color: sugar.background_color,
                size: [self.sugar_dimension.0, self.sugar_dimension.0],
            });

            x += (self.sugar_dimension.0)/ 2.;
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

        println!("{:?}", self.acc_line_y);

        self.acc_line_y = (style.screen_position.1 + self.acc_line) / 2.;
        self.acc_line += style.text_scale;
    }

    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    #[inline]
    pub fn init(&mut self, color: wgpu::Color, style: SugarloafStyle) {
        let mut encoder =
            self.ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("sugarloaf::init -> Create command encoder"),
                });
        let frame = self
            .ctx
            .surface
            .get_current_texture()
            .expect("Get next frame");
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

        if self.sugar_dimension == (0., 0.) {
            let text = vec![OwnedText::new(' ')
                .with_font_id(FontId(0))
                .with_color([0.,0.,0.,0.])
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

            // TODO: Now Sugarloaf depends of run an init operation
            // and also font has same size for get dimensions.
            if let Some(rect) = self.text_brush.glyph_bounds(section) {
                let width = rect.max.x - rect.min.x;
                let height = rect.max.y - rect.min.y;

                println!("{:?} {:?}", width, height);
                self.sugar_dimension = (width, height);

            };
        }

        self.ctx.staging_belt.finish();
        self.ctx.queue.submit(Some(encoder.finish()));
        frame.present();
        self.ctx.staging_belt.recall();
    }

    fn reset_state(&mut self) {
        self.acc_line = 0.0;
        self.acc_line_y = -0.175;
    }

    pub fn pile_rect(&mut self, instances: Vec<Rect>) -> &mut Self {
        self.rects = instances;
        self
    }

    #[inline]
    pub fn render(&mut self, color: colors::ColorWGPU) {
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
                    label: Some("Clear background frame"),
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
