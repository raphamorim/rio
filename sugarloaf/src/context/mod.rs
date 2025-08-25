use crate::sugarloaf::{Colorspace, SugarloafWindow, SugarloafWindowSize};
use crate::SugarloafRenderer;

pub struct Context<'a> {
    pub device: wgpu::Device,
    pub surface: wgpu::Surface<'a>,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
    pub size: SugarloafWindowSize,
    pub scale: f32,
    alpha_mode: wgpu::CompositeAlphaMode,
    pub adapter_info: wgpu::AdapterInfo,
    surface_caps: wgpu::SurfaceCapabilities,
    pub supports_f16: bool,
    pub colorspace: Colorspace,
}

#[inline]
#[cfg(not(target_os = "macos"))]
fn find_best_texture_format(
    formats: &[wgpu::TextureFormat],
    colorspace: Colorspace,
) -> wgpu::TextureFormat {
    #[cfg(windows)]
    let unsupported_formats = [
        wgpu::TextureFormat::Rgba8Snorm,
        wgpu::TextureFormat::Rgba16Float,
    ];

    #[cfg(not(windows))]
    let unsupported_formats = [wgpu::TextureFormat::Rgba8Snorm];

    if let Some(format) = formats
        .iter()
        .copied()
        .find(|f| wgpu::TextureFormat::is_srgb(f) && !unsupported_formats.contains(f))
    {
        tracing::info!(
            "Sugarloaf selected sRGB format: {format:?} from {:?} for colorspace {:?}",
            formats,
            colorspace
        );
        return format;
    }

    let format = formats
        .iter()
        .copied()
        .find(|f| !unsupported_formats.contains(f))
        .unwrap_or(formats[0]);

    tracing::info!(
        "Sugarloaf selected format: {format:?} from {:?} for colorspace {:?}",
        formats,
        colorspace
    );

    format
}

#[inline]
#[cfg(target_os = "macos")]
fn get_macos_texture_format(colorspace: Colorspace) -> wgpu::TextureFormat {
    match colorspace {
        Colorspace::Srgb => wgpu::TextureFormat::Bgra8UnormSrgb,
        Colorspace::DisplayP3 | Colorspace::Rec2020 => wgpu::TextureFormat::Bgra8Unorm,
    }
}

impl Context<'_> {
    pub fn new<'a>(
        sugarloaf_window: SugarloafWindow,
        renderer_config: SugarloafRenderer,
    ) -> Context<'a> {
        // The backend can be configured using the `WGPU_BACKEND`
        // environment variable. If the variable is not set, the primary backend
        // will be used. The following values are allowed:
        // - `vulkan`
        // - `metal`
        // - `dx12`
        // - `dx11`
        // - `gl`
        // - `webgpu`
        // - `primary`
        let backend = wgpu::Backends::from_env().unwrap_or(renderer_config.backend);
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: backend,
            ..Default::default()
        });

        tracing::info!("selected instance: {instance:?}");

        #[cfg(not(target_arch = "wasm32"))]
        {
            tracing::info!("Available adapters:");
            for a in instance.enumerate_adapters(wgpu::Backends::all()) {
                tracing::info!("    {:?}", a.get_info())
            }
        }

        tracing::info!("initializing the surface");

        let size = sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        let surface: wgpu::Surface<'a> =
            instance.create_surface(sugarloaf_window).unwrap();
        let adapter = futures::executor::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: renderer_config.power_preference,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ))
        .expect("Request adapter");

        let adapter_info = adapter.get_info();
        tracing::info!("Selected adapter: {:?}", adapter_info);

        let surface_caps = surface.get_capabilities(&adapter);

        #[cfg(target_os = "macos")]
        let format = get_macos_texture_format(renderer_config.colorspace);
        #[cfg(not(target_os = "macos"))]
        let format = find_best_texture_format(
            surface_caps.formats.as_slice(),
            renderer_config.colorspace,
        );

        let (device, queue, supports_f16) = {
            {
                if let Ok(result) = futures::executor::block_on(adapter.request_device(
                    // ADDRESS_MODE_CLAMP_TO_BORDER is required for librashader
                    // SHADER_F16 enables half precision floating point support
                    &wgpu::DeviceDescriptor {
                        required_features: wgpu::Features::empty()
                            | wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER
                            | wgpu::Features::SHADER_F16,
                        ..Default::default()
                    },
                )) {
                    (result.0, result.1, true)
                } else {
                    // Fallback without f16 support for compatibility
                    if let Ok(result) = futures::executor::block_on(
                        adapter.request_device(&wgpu::DeviceDescriptor {
                            required_features: wgpu::Features::empty()
                                | wgpu::Features::ADDRESS_MODE_CLAMP_TO_BORDER,
                            ..Default::default()
                        }),
                    ) {
                        (result.0, result.1, false)
                    } else {
                        // These downlevel limits will allow the code to run on all possible hardware
                        let result = futures::executor::block_on(adapter.request_device(
                            &wgpu::DeviceDescriptor {
                                memory_hints: wgpu::MemoryHints::Performance,
                                label: None,
                                required_features: wgpu::Features::empty(),
                                required_limits: wgpu::Limits::downlevel_webgl2_defaults(
                                ),
                                ..Default::default()
                            },
                        ))
                        .expect("Request device");
                        (result.0, result.1, false)
                    }
                }
            }
        };

        let alpha_mode = if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PostMultiplied)
        {
            wgpu::CompositeAlphaMode::PostMultiplied
        } else if surface_caps
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::PreMultiplied)
        {
            wgpu::CompositeAlphaMode::PreMultiplied
        } else {
            wgpu::CompositeAlphaMode::Auto
        };

        // Configure view formats for wide color gamut support
        let view_formats = match renderer_config.colorspace {
            Colorspace::DisplayP3 | Colorspace::Rec2020 => {
                // For wide color gamut, we may want to support additional view formats
                // This allows the surface to be viewed in different formats
                vec![format]
            }
            Colorspace::Srgb => {
                vec![]
            }
        };

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: Self::get_texture_usage(&surface_caps),
                format,
                width: size.width as u32,
                height: size.height as u32,
                view_formats,
                alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );

        tracing::info!("F16 shader support: {}", supports_f16);
        tracing::info!("Configured colorspace: {:?}", renderer_config.colorspace);
        tracing::info!("Surface format: {:?}", format);

        Context {
            device,
            queue,
            surface,
            format,
            alpha_mode,
            size: SugarloafWindowSize {
                width: size.width,
                height: size.height,
            },
            scale,
            adapter_info,
            surface_caps,
            supports_f16,
            colorspace: renderer_config.colorspace,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.size.width = width as f32;
        self.size.height = height as f32;

        // Configure view formats for wide color gamut support
        let view_formats = match self.colorspace {
            Colorspace::DisplayP3 | Colorspace::Rec2020 => {
                vec![self.format]
            }
            Colorspace::Srgb => {
                vec![]
            }
        };

        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: Self::get_texture_usage(&self.surface_caps),
                format: self.format,
                width,
                height,
                view_formats,
                alpha_mode: self.alpha_mode,
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
            },
        );
    }

    pub fn surface_caps(&self) -> &wgpu::SurfaceCapabilities {
        &self.surface_caps
    }

    pub fn supports_f16(&self) -> bool {
        self.supports_f16
    }

    pub fn get_optimal_texture_format(&self, channels: u32) -> wgpu::TextureFormat {
        if self.supports_f16 {
            match channels {
                1 => wgpu::TextureFormat::R16Float,
                2 => wgpu::TextureFormat::Rg16Float,
                4 => wgpu::TextureFormat::Rgba16Float,
                _ => wgpu::TextureFormat::Rgba8Unorm, // fallback
            }
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        }
    }

    pub fn get_optimal_texture_sample_type(&self) -> wgpu::TextureSampleType {
        // Both Rgba16Float (f16) and Rgba8Unorm (f32) use Float sample type with filtering
        wgpu::TextureSampleType::Float { filterable: true }
    }

    pub fn convert_rgba8_to_optimal_format(&self, rgba8_data: &[u8]) -> Vec<u8> {
        if self.supports_f16 {
            // Convert u8 RGBA to f16 RGBA
            let mut f16_data = Vec::with_capacity(rgba8_data.len() * 2);
            for chunk in rgba8_data.chunks(4) {
                if chunk.len() == 4 {
                    // Convert u8 [0-255] to f16 [0.0-1.0]
                    let r = half::f16::from_f32(chunk[0] as f32 / 255.0);
                    let g = half::f16::from_f32(chunk[1] as f32 / 255.0);
                    let b = half::f16::from_f32(chunk[2] as f32 / 255.0);
                    let a = half::f16::from_f32(chunk[3] as f32 / 255.0);

                    f16_data.extend_from_slice(&r.to_le_bytes());
                    f16_data.extend_from_slice(&g.to_le_bytes());
                    f16_data.extend_from_slice(&b.to_le_bytes());
                    f16_data.extend_from_slice(&a.to_le_bytes());
                }
            }
            f16_data
        } else {
            rgba8_data.to_vec()
        }
    }

    fn get_texture_usage(caps: &wgpu::SurfaceCapabilities) -> wgpu::TextureUsages {
        let mut usage = wgpu::TextureUsages::RENDER_ATTACHMENT;

        // COPY_DST and COPY_SRC are required for FiltersBrush
        // But some backends like OpenGL might not support COPY_DST and COPY_SRC
        // https://github.com/emilk/egui/pull/3078

        if caps.usages.contains(wgpu::TextureUsages::COPY_DST) {
            usage |= wgpu::TextureUsages::COPY_DST;
        }

        if caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }

        usage
    }
}
