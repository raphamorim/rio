use crate::components::row::{Quad, Row};
use crate::components::text;
use crate::context::Context;
use crate::core::SugarStack;
use crate::font::Font;
use glyph_brush::ab_glyph::Font as GFont;
use glyph_brush::ab_glyph::{self, FontArc};
use glyph_brush::FontId;
use glyph_brush::{OwnedSection, OwnedText};

#[derive(Default, Copy, Clone)]
pub struct SugarloafStyle {
    pub screen_position: (f32, f32),
    pub bounds: (f32, f32),
    pub text_scale: f32,
}

// TODO: Use macro instead
pub enum RendererTarget {
    Desktop,
    Web,
}

pub fn orthographic_projection(_width: u32, _height: u32) -> [f32; 16] {
    // [0.0016666667, 0.0, 0.0, 0.0, 0.0, -0.0025, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -1.0, 1.0, -0.0, 1.0]

    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, -0.5, 1.45, -0.0,
        1.0,
    ]
    // [
    //     2.0 / width as f32,
    //     0.0,
    //     0.0,
    //     0.0,
    //     0.0,
    //     -2.0 / height as f32,
    //     0.0,
    //     0.0,
    //     0.0,
    //     0.0,
    //     1.0,
    //     0.0,
    //     -1.0,
    //     1.0,
    //     0.0,
    //     1.0,
    // ]
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
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        staging_belt: &mut wgpu::util::StagingBelt,
        transform: [f32; 16],
        instances: &[Quad],
    );
}

pub struct Sugarloaf {
    pub ctx: Context,
    brush: text::GlyphBrush<()>,
    #[allow(dead_code)]
    row: Row,
    rows: Vec<Quad>,
    acc_line: f32,
    acc_line_y: f32,
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
                let brush = text::GlyphBrushBuilder::using_fonts(vec![
                    font.system,
                    font.symbol,
                    font.emojis,
                    font.unicode,
                ])
                .build(&ctx.device, ctx.format);
                let row = Row::init(&ctx);
                Ok(Sugarloaf {
                    ctx,
                    row,
                    rows: vec![],
                    brush,
                    acc_line: 0.0,
                    acc_line_y: -0.175,
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
        let mut x = 0.030;

        let fonts = self.brush.fonts();
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

            self.rows.push(Quad {
                position: [x, self.acc_line_y],
                color: sugar.background_color,
                size: [0.14, 0.14],
            });

            x += 0.0242;
        }

        self.acc_line_y += -0.0734;

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

        // println!("{:?}", self.brush.glyph_bounds(section));

        self.brush.queue(section);

        self.acc_line += style.text_scale;
    }

    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    #[inline]
    pub fn skeleton(&mut self, color: wgpu::Color) {
        let mut encoder =
            self.ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Sugarloaf skeleton"),
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
            label: Some("Render -> Clear frame"),
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
        self.ctx.staging_belt.finish();
        self.ctx.queue.submit(Some(encoder.finish()));
        frame.present();
        self.ctx.staging_belt.recall();
    }

    fn reset_state(&mut self) {
        self.acc_line = 0.0;
        self.acc_line_y = -0.175;
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

                // let _ = self.row.render(
                //     &mut encoder,
                //     &self.ctx.device,
                //     view,
                //     &mut self.ctx.staging_belt,
                //     orthographic_projection(self.ctx.size.width, self.ctx.size.height),
                //     &self.rows,
                // );

                self.rows = vec![];

                let _ = self.brush.draw_queued(
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

pub struct CustomRenderer<'a, R: Renderable> {
    pub ctx: Context,
    queue: Vec<&'a mut R>,
}

impl<'a, R: Renderable> CustomRenderer<'a, R> {
    pub async fn new(
        target: RendererTarget,
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
    ) -> CustomRenderer<R> {
        let ctx = match target {
            RendererTarget::Desktop => Context::new(winit_window, power_preference).await,
            RendererTarget::Web => {
                todo!("web not implemented");
            }
        };

        CustomRenderer { ctx, queue: vec![] }
    }

    pub fn add_component(&mut self, renderable_item: &'a mut R)
    where
        R: Renderable,
    {
        self.queue.push(renderable_item);
    }

    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    pub fn start(&self) {}

    pub fn render(&mut self) {
        match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let instances = [
                    Quad {
                        position: [0.0, 0.0],
                        color: [1.0, 1.0, 0.0, 1.0],
                        size: [0.0, 0.0],
                    },
                    Quad {
                        position: [0.025, 0.0],
                        color: [0.0, 1.0, 0.0, 1.0],
                        size: [0.0, 0.0],
                    },
                    Quad {
                        position: [0.045, 0.0],
                        color: [0.0, 1.0, 1.0, 1.0],
                        size: [0.0, 0.0],
                    },
                    Quad {
                        position: [0.0, -0.05],
                        color: [0.0, 0.5, 1.0, 1.0],
                        size: [0.0, 0.0],
                    },
                    Quad {
                        position: [0.025, -0.05],
                        color: [1.0, 0.0, 0.0, 1.0],
                        size: [0.0, 0.0],
                    },
                    Quad {
                        position: [0.045, -0.05],
                        color: [0.5, 1.0, 1.0, 1.0],
                        size: [0.0, 0.0],
                    },
                ];

                if !self.queue.is_empty() {
                    for item in self.queue.iter_mut() {
                        item.render(
                            &mut encoder,
                            &self.ctx.device,
                            view,
                            &mut self.ctx.staging_belt,
                            orthographic_projection(
                                self.ctx.size.width,
                                self.ctx.size.height,
                            ),
                            &instances,
                        );
                    }
                }

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
