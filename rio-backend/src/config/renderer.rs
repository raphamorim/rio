use serde::{Deserialize, Serialize};
use std::fmt::Display;
// `Filter` is wgpu-only (librashader runtime is wgpu-only upstream).
// Gated together with the `wgpu` feature.
#[cfg(feature = "wgpu")]
use sugarloaf::Filter;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Renderer {
    #[serde(default = "Backend::default", skip_serializing)]
    pub backend: Backend,
    #[serde(default = "bool::default", rename = "disable-unfocused-render")]
    pub disable_unfocused_render: bool,
    #[serde(
        default = "default_disable_occluded_render",
        rename = "disable-occluded-render"
    )]
    pub disable_occluded_render: bool,
    #[serde(default = "Vec::default")]
    #[cfg(feature = "wgpu")]
    pub filters: Vec<Filter>,
    #[serde(default = "RendererStategy::default")]
    pub strategy: RendererStategy,
    /// Use the CPU rasterizer (tiny-skia) instead of the GPU pipeline.
    /// Experimental. v1 supports solid quads + glyphs only; image
    /// overlays, GPU filters, advanced underline styles, and corner radii
    /// are not yet implemented on the CPU path.
    #[serde(default = "default_use_cpu", rename = "use-cpu")]
    pub use_cpu: bool,
}

fn default_use_cpu() -> bool {
    false
}

fn default_disable_occluded_render() -> bool {
    false
}

#[derive(Default, Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum RendererStategy {
    #[default]
    #[serde(alias = "events")]
    Events,
    #[serde(alias = "game")]
    Game,
}

impl RendererStategy {
    #[inline]
    pub fn is_game(&self) -> bool {
        self == &RendererStategy::Game
    }

    #[inline]
    pub fn is_event_based(&self) -> bool {
        self == &RendererStategy::Events
    }
}

#[allow(clippy::derivable_impls)]
impl Default for Renderer {
    fn default() -> Renderer {
        Renderer {
            backend: Backend::default(),
            disable_unfocused_render: false,
            disable_occluded_render: default_disable_occluded_render(),
            #[cfg(feature = "wgpu")]
            filters: Vec::default(),
            strategy: RendererStategy::Events,
            use_cpu: default_use_cpu(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub enum Backend {
    /// Native Metal (macOS only).
    #[cfg(target_os = "macos")]
    #[cfg_attr(target_os = "macos", default)]
    #[serde(alias = "metal")]
    Metal,
    /// Native Vulkan on Linux; wgpu Vulkan elsewhere (requires the
    /// `wgpu` feature).
    #[cfg_attr(target_os = "linux", default)]
    #[serde(alias = "vulkan")]
    Vulkan,
    /// wgpu umbrella backend — wgpu picks the best available native
    /// API (Metal / Vulkan / DX12 / GL / WebGPU). Requires the `wgpu`
    /// feature.
    #[cfg_attr(not(any(target_os = "macos", target_os = "linux")), default)]
    #[serde(alias = "webgpu", alias = "wgpu")]
    Webgpu,
}

impl Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            #[cfg(target_os = "macos")]
            Backend::Metal => write!(f, "Metal"),
            Backend::Vulkan => write!(f, "Vulkan"),
            Backend::Webgpu => write!(f, "Webgpu"),
        }
    }
}
