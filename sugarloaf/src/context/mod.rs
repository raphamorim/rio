pub mod metal;
pub mod webgpu;

use crate::sugarloaf::{SugarloafBackend, SugarloafWindow};
use crate::SugarloafRenderer;

pub struct Context<'a> {
    pub inner: ContextType<'a>,
}

pub enum ContextType<'a> {
    Wgpu(webgpu::WgpuContext<'a>),
    Metal(metal::MetalContext),
}

impl Context<'_> {
    pub fn new<'a>(
        sugarloaf_window: SugarloafWindow,
        renderer_config: SugarloafRenderer,
    ) -> Context<'a> {
        let inner = match renderer_config.backend {
            SugarloafBackend::Wgpu(backends) => ContextType::Wgpu(
                webgpu::WgpuContext::new(sugarloaf_window, renderer_config, backends),
            ),
            SugarloafBackend::Metal => ContextType::Metal(metal::MetalContext::new()),
        };

        Context { inner }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        match &mut self.inner {
            ContextType::Wgpu(ctx) => {
                ctx.resize(width, height);
            }
            ContextType::Metal(_) => {}
        }
    }
}
