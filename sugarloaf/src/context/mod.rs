pub mod cpu;
#[cfg(target_os = "macos")]
pub mod metal;
#[cfg(target_os = "linux")]
pub mod vulkan;
#[cfg(feature = "wgpu")]
pub mod webgpu;

use crate::sugarloaf::{SugarloafBackend, SugarloafWindow};
use crate::{SugarloafRenderer, SugarloafWindowSize};

pub struct Context<'a> {
    pub inner: ContextType<'a>,
}

#[allow(clippy::large_enum_variant)]
pub enum ContextType<'a> {
    #[cfg(feature = "wgpu")]
    Wgpu(webgpu::WgpuContext<'a>),
    #[cfg(target_os = "macos")]
    Metal(metal::MetalContext),
    #[cfg(target_os = "linux")]
    Vulkan(vulkan::VulkanContext),
    Cpu(cpu::CpuContext),
    /// Lifetime placeholder for the Wgpu variant when it's
    /// feature-gated out — keeps `'a` referenced across the enum so
    /// the compiler doesn't complain about an unused parameter on
    /// builds without wgpu.
    #[cfg(not(feature = "wgpu"))]
    #[doc(hidden)]
    _Phantom(std::marker::PhantomData<&'a ()>),
}

impl Context<'_> {
    pub fn new<'a>(
        sugarloaf_window: SugarloafWindow,
        renderer_config: SugarloafRenderer,
    ) -> Context<'a> {
        let inner = match renderer_config.backend {
            #[cfg(feature = "wgpu")]
            SugarloafBackend::Wgpu(backends) => ContextType::Wgpu(
                webgpu::WgpuContext::new(sugarloaf_window, renderer_config, backends),
            ),
            #[cfg(target_os = "macos")]
            SugarloafBackend::Metal => {
                ContextType::Metal(metal::MetalContext::new(sugarloaf_window))
            }
            #[cfg(target_os = "linux")]
            SugarloafBackend::Vulkan => {
                ContextType::Vulkan(vulkan::VulkanContext::new(sugarloaf_window))
            }
            SugarloafBackend::Cpu => {
                ContextType::Cpu(cpu::CpuContext::new(sugarloaf_window))
            }
        };

        Context { inner }
    }

    #[inline]
    pub fn scale(&self) -> f32 {
        match &self.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(ctx) => ctx.scale,
            #[cfg(target_os = "macos")]
            ContextType::Metal(ctx) => ctx.scale,
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(ctx) => ctx.scale,
            ContextType::Cpu(ctx) => ctx.scale,
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        }
    }

    #[inline]
    pub fn set_scale(&mut self, scale: f32) {
        match &mut self.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(ctx) => {
                ctx.set_scale(scale);
            }
            #[cfg(target_os = "macos")]
            ContextType::Metal(ctx) => {
                ctx.set_scale(scale);
            }
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(ctx) => {
                ctx.set_scale(scale);
            }
            ContextType::Cpu(ctx) => {
                ctx.set_scale(scale);
            }
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        }
    }

    #[inline]
    pub fn size(&self) -> SugarloafWindowSize {
        match &self.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(ctx) => ctx.size,
            #[cfg(target_os = "macos")]
            ContextType::Metal(ctx) => ctx.size,
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(ctx) => ctx.size,
            ContextType::Cpu(ctx) => ctx.size,
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        match &mut self.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(ctx) => ctx.resize(width, height),
            #[cfg(target_os = "macos")]
            ContextType::Metal(ctx) => ctx.resize(width, height),
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(ctx) => ctx.resize(width, height),
            ContextType::Cpu(ctx) => ctx.resize(width, height),
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        }
    }

    #[inline]
    pub fn supports_f16(&self) -> bool {
        match &self.inner {
            #[cfg(feature = "wgpu")]
            ContextType::Wgpu(ctx) => ctx.supports_f16(),
            #[cfg(target_os = "macos")]
            ContextType::Metal(ctx) => ctx.supports_f16(),
            #[cfg(target_os = "linux")]
            ContextType::Vulkan(ctx) => ctx.supports_f16(),
            ContextType::Cpu(ctx) => ctx.supports_f16(),
            #[cfg(not(feature = "wgpu"))]
            ContextType::_Phantom(_) => unreachable!(),
        }
    }
}
