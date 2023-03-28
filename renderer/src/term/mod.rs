mod cache;
use crate::RendererStyles;
use std::rc::Rc;
use cache::Cache;

use std::error::Error;

pub struct TermStyles {
    size: (u32, u32),
    scale: f64
}

pub struct Term {
    format: wgpu::TextureFormat,
    alpha_mode: wgpu::CompositeAlphaMode,
    cache: Cache,
}

impl Term {
    pub async fn new(
        style: TermStyles
    ) -> Result<Term, Box<dyn Error>> {
        let size = winit_window.inner_size();

        // let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        //     label: Some("Shader"),
        //     source: wgpu::ShaderSource::Wgsl(include_str!("../bar/bar.wgsl").into()),
        // });

        let scale = winit_window.scale_factor() as f32;
        // let bar: BarBrush = BarBrush::new(&device, shader, scale);
        let caps = surface.get_capabilities(&adapter);
        // Possible formats for MacOs:
        // [Bgra8UnormSrgb, Bgra8Unorm, Rgba16Float, Rgb10a2Unorm]
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");
        let alpha_modes = caps.alpha_modes;
        let alpha_mode = alpha_modes[0];

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width,
                height: size.height,
                view_formats: vec![],
                alpha_mode,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        let renderer_styles = RendererStyles::new(
            size.width as f32,
            size.height as f32,
            scale,
            config.style.font_size,
        );

        // let renderer = match Renderer::new(&device, format, config, renderer_styles) {
        //     Ok(r) => r,
        //     Err(e) => panic!("{e:?}"),
        // };

        let cache = Cache::new(&device, 1024, 1024);

        let shell: String = match std::env::var("SHELL") {
            Ok(val) => val,
            Err(..) => String::from("bash"),
        };

        // let pty_: teletypewriter::Pty = pty(
        //     &Cow::Borrowed(&shell),
        //     renderer.config.columns,
        //     renderer.config.rows,
        // );

        // let columns = renderer.config.columns;
        // let rows = renderer.config.rows;

        // let machine = Machine::new(pty_, columns.into(), rows.into());

        // machine.spawn();

        Ok(Term {
            // renderer,
            format,
            alpha_mode,
            cache,
        })
    }

    // #[inline]
    // fn configure_surface(&mut self) {
    //     self.surface.configure(
    //         &self.device,
    //         &wgpu::SurfaceConfiguration {
    //             usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
    //             format: self.format,
    //             width: self.size.width,
    //             height: self.size.height,
    //             view_formats: vec![],
    //             alpha_mode: self.alpha_mode,
    //             present_mode: wgpu::PresentMode::AutoVsync,
    //         },
    //     );
    // }

    #[inline]
    fn create_encoder(&self) -> wgpu::CommandEncoder {
        self.device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Redraw"),
            })
    }

    // TODO: Asynchronous update based on 2s
    // Idea? Prob move Render inside of RenderUi that contains Tabs/Render
    // Allowing switch Renders
    // fn get_command_name(&self) -> String {
    //     // format!("■ {:?}", teletypewriter::command_per_pid(self.pid))
    //     format!(
    //         "{} zsh ",
    //         self.renderer.config.advanced.tab_character_active
    //     )
    // }

    pub fn draw(&mut self, surface: wgpu::Surface, queue: wgpu::Queue) {
        let mut encoder = self.create_encoder();

        let frame = surface.get_current_texture().expect("Get next frame");
        let view = &frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        //     label: Some("Render -> Clear frame"),
        //     color_attachments: &[Some(wgpu::RenderPassColorAttachment {
        //         view,
        //         resolve_target: None,
        //         ops: wgpu::Operations {
        //             load: wgpu::LoadOp::Clear(self.renderer.config.colors.background.1),
        //             store: true,
        //         },
        //     })],
        //     depth_stencil_attachment: None,
        // });

        // self.renderer.topbar(self.windows_title_arc.lock().unwrap().to_string());
        // self.renderer
        //     .term(self.visible_rows_arc.lock().unwrap().to_vec());

        // self.renderer
        //     .brush
        //     .draw_queued(
        //         &self.device,
        //         &mut self.staging_belt,
        //         &mut encoder,
        //         view,
        //         (self.size.width, self.size.height),
        //     )
        //     .expect("Draw queued");

        self.staging_belt.finish();
        queue.submit(Some(encoder.finish()));
        frame.present();

        // Recall unused staging buffers
        self.staging_belt.recall();
    }
}
