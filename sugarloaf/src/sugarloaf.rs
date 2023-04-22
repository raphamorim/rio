use crate::components::text;
use glyph_brush::ab_glyph::FontArc;
use glyph_brush::GlyphCruncher;
use glyph_brush::{OwnedSection, OwnedText};
use crate::context::Context;
use crate::core::{ empty_sugar_pile, SugarPile, SugarStack, Sugar };

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
    fn queue_render(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        view: &wgpu::TextureView,
        queue: &wgpu::Queue,
        staging_belt: &mut wgpu::util::StagingBelt,
    );
}

pub struct Sugarloaf {
    pub ctx: Context,
    brush: text::GlyphBrush<()>,
    pile: SugarPile,
    acc_line: f32,
}

// TODO: Sugarloaf integrate CustomRenderer (or Renderer) layer usage
impl Sugarloaf {
    pub async fn new(
        target: RendererTarget,
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
    ) -> Result<Sugarloaf, String> {
        let ctx = match target {
            RendererTarget::Desktop => Context::new(winit_window, power_preference).await,
            RendererTarget::Web => {
                todo!("web not implemented");
            }
        };

        match FontArc::try_from_slice(crate::shared::FONT_FIRAMONO) {
            Ok(font_data) => {
                let brush =
                    text::GlyphBrushBuilder::using_font(font_data).build(&ctx.device, ctx.format);
                // let quad = quad::Pipeline::new(&device, format);
                // let fps = frames::Counter::new();
                // let scene = Scene::new(&device, format);
                Ok(Sugarloaf {
                    ctx,
                    // scene,
                    brush,
                    acc_line: 0.0,
                    pile: empty_sugar_pile()
                })
            }
            Err(err_message) => Err(format!(
                "Renderer error: could not initialize font {err_message:?}"
            )),
        }
    }

    // pub fn update_size() -> RetType {
        
    // }

    pub fn stack(&mut self, stack: SugarStack, style: SugarloafStyle) {
        let mut text: Vec<OwnedText> = vec![];
        for sugar in stack.iter() {
            text.push(OwnedText::new(sugar.content.to_owned())
                .with_color(sugar.foreground_color)
                .with_scale(style.text_scale));
        }

        self.brush.queue(&OwnedSection {
            screen_position: (
                style.screen_position.0,
                style.screen_position.1 + self.acc_line,
            ),
            bounds: style.bounds,
            text: text,
            layout: glyph_brush::Layout::default_single_line()
                .v_align(glyph_brush::VerticalAlign::Bottom),
        });

        self.acc_line = self.acc_line + style.text_scale;

        // println!("{:?}", self.brush.glyph_bounds(section));
    }

    pub fn get_context(&self) -> &Context {
        &self.ctx
    }

    // #[inline]
    // pub fn term(&mut self, rows: Vec<Row<Square>>, style: Style) {
    //     let mut line_height: f32 = 0.0;
    //     let cursor_row = self.cursor.position.1;
    //     for (i, row) in rows.iter().enumerate() {
    //         self.render_row(row, style, line_height, cursor_row == i);
    //         line_height += style.text_scale;
    //     }
    // }

    #[inline]
    pub fn skeleton(&mut self, color: wgpu::Color) {
        // TODO: WGPU caching
        let mut encoder =
            self.ctx
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Skeleton"),
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

                let _ =
                    self.brush
                        .draw_queued(&self.ctx.device, &mut self.ctx.staging_belt, &mut encoder, view, (self.ctx.size.width, self.ctx.size.height));

                self.ctx.staging_belt.finish();
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
                self.ctx.staging_belt.recall();
            }
            Err(error) => match error {
                wgpu::SurfaceError::OutOfMemory => {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
                _ => {
                    // Wait for rendering next frame.
                }
            },
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

                if self.queue.len() > 0 {
                    for item in self.queue.iter_mut() {
                        item.queue_render(
                            &mut encoder,
                            &self.ctx.device,
                            view,
                            &mut self.ctx.queue,
                            &mut self.ctx.staging_belt,
                        );
                    }
                }

                self.ctx.staging_belt.finish();
                self.ctx.queue.submit(Some(encoder.finish()));
                frame.present();
                self.ctx.staging_belt.recall();
            }
            Err(error) => match error {
                wgpu::SurfaceError::OutOfMemory => {
                    panic!("Swapchain error: {error}. Rendering cannot continue.")
                }
                _ => {
                    // Wait for rendering next frame.
                }
            },
        }
    }
}
