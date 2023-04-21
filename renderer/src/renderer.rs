use crate::renderable::Renderable;
use crate::context::Context;

pub enum RendererTarget {
	Desktop,
	Web
}

pub struct Renderer<'a, R: Renderable> {
	pub ctx: Context,
	queue: Vec<&'a mut R>
}

impl<'a, R: Renderable> Renderer<'a, R> {
	pub async fn new(
		target: RendererTarget,
		winit_window: &winit::window::Window,
		power_preference: wgpu::PowerPreference) -> Renderer<R> {
		let ctx = match target {
			RendererTarget::Desktop => Context::new(winit_window, power_preference).await,
			RendererTarget::Web => { todo!("web not implemented");}
		};

		Renderer {
			ctx,
			queue: vec![]
		}
	}

	pub fn add_component(&mut self, renderable_item: &'a mut R)
	where R: Renderable {
		self.queue.push(renderable_item);
	}

	pub fn get_context(&self) -> &Context {
		&self.ctx
	}

	pub fn render(&mut self) {
		match self.ctx.surface.get_current_texture() {
            Ok(frame) => {
                let mut encoder = self.ctx.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor { label: None },
                );

                let view = &frame
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                for item in self.queue.iter_mut() {
			    	item.queue_render(&mut encoder, &self.ctx.device, view, &mut self.ctx.queue, &mut self.ctx.staging_belt);
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