#[derive(Debug)]
pub struct Context {
    pub device: wgpu::Device,
    pub surface: wgpu::Surface,
    pub queue: wgpu::Queue,
    pub staging_belt: wgpu::util::StagingBelt,
    pub format: wgpu::TextureFormat,
    pub size: winit::dpi::PhysicalSize<u32>,
    pub scale: f32,
}

impl Context {
    pub async fn new(
        winit_window: &winit::window::Window,
        power_preference: wgpu::PowerPreference,
    ) -> Context {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            // dx12_shader_compiler: wgpu::Dx12Compiler::Fxc,
            ..Default::default()
        });

        let size = winit_window.inner_size();
        let scale = winit_window.scale_factor();

        let surface: wgpu::Surface =
            unsafe { instance.create_surface(&winit_window).unwrap() };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Request adapter");

        let caps = surface.get_capabilities(&adapter);
        let formats = caps.formats;
        let format = *formats.last().expect("No supported formats for surface");

        let (device, queue) = (async {
            adapter
                .request_device(&wgpu::DeviceDescriptor::default(), None)
                .await
                .expect("Request device")
        })
        .await;

        let staging_belt = wgpu::util::StagingBelt::new(5 * 1024);

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format,
                width: size.width,
                height: size.height,
                view_formats: vec![],
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );

        Context {
            device,
            queue,
            surface,
            staging_belt,
            format,
            size,
            scale: scale as f32,
        }
    }

    pub fn update_size(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: self.format,
                width: size.width,
                height: size.height,
                view_formats: vec![],
                alpha_mode: wgpu::CompositeAlphaMode::Auto,
                present_mode: wgpu::PresentMode::AutoVsync,
            },
        );
    }
}
