use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};

#[cfg(target_os = "macos")]
pub mod metal;

pub mod webgpu;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBackend {
    #[default]
    WebGpu,
    #[cfg(target_os = "macos")]
    Metal,
}

pub trait RenderContext {
    type Device;
    type Queue;
    type Surface;
    type TextureFormat;
    type CommandEncoder;
    type RenderPass<'a>;
    type Texture;
    type TextureView;
    type Buffer;
    type BindGroup;
    type RenderPipeline;
    type ComputePipeline;
    type Sampler;

    fn new(window: SugarloafWindow, backend: RenderBackend) -> Self;
    fn resize(&mut self, width: u32, height: u32);
    fn get_current_texture(&self) -> Result<Self::Texture, String>;
    fn create_command_encoder(&self) -> Self::CommandEncoder;
    fn submit_commands(&self, encoder: Self::CommandEncoder);
    fn present_texture(&self, texture: Self::Texture);
    fn size(&self) -> SugarloafWindowSize;
    fn scale(&self) -> f32;
    fn supports_f16(&self) -> bool;
}

pub enum BackendContext<'a> {
    WebGpu(webgpu::WebGpuContext<'a>),
    #[cfg(target_os = "macos")]
    Metal(metal::MetalContext),
}

impl<'a> BackendContext<'a> {
    pub fn new(window: SugarloafWindow, backend: RenderBackend) -> Self {
        match backend {
            RenderBackend::WebGpu => {
                BackendContext::WebGpu(webgpu::WebGpuContext::new(window, backend))
            }
            #[cfg(target_os = "macos")]
            RenderBackend::Metal => {
                BackendContext::Metal(metal::MetalContext::new(window, backend))
            }
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match self {
            BackendContext::WebGpu(ctx) => ctx.resize(width, height),
            #[cfg(target_os = "macos")]
            BackendContext::Metal(ctx) => ctx.resize(width, height),
        }
    }

    pub fn size(&self) -> SugarloafWindowSize {
        match self {
            BackendContext::WebGpu(ctx) => ctx.size(),
            #[cfg(target_os = "macos")]
            BackendContext::Metal(ctx) => ctx.size(),
        }
    }

    pub fn scale(&self) -> f32 {
        match self {
            BackendContext::WebGpu(ctx) => ctx.scale(),
            #[cfg(target_os = "macos")]
            BackendContext::Metal(ctx) => ctx.scale(),
        }
    }

    pub fn supports_f16(&self) -> bool {
        match self {
            BackendContext::WebGpu(ctx) => ctx.supports_f16(),
            #[cfg(target_os = "macos")]
            BackendContext::Metal(ctx) => ctx.supports_f16(),
        }
    }
}
