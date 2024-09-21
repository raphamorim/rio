use crate::config::default_frame_time;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Renderer {
    #[serde(default = "Performance::default")]
    pub performance: Performance,
    #[serde(default = "Backend::default", skip_serializing)]
    pub backend: Backend,
    #[serde(default = "bool::default", rename = "disable-unfocused-render")]
    pub disable_unfocused_render: bool,
    #[serde(default = "default_frame_time", rename = "frame-time")]
    pub frame_time: u64,
}

#[allow(clippy::derivable_impls)]
impl Default for Renderer {
    fn default() -> Renderer {
        Renderer {
            performance: Performance::default(),
            backend: Backend::default(),
            disable_unfocused_render: false,
            frame_time: default_frame_time(),
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum Performance {
    #[default]
    #[serde(alias = "high")]
    High,
    #[serde(alias = "low")]
    Low,
}

impl Display for Performance {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Performance::High => {
                write!(f, "High")
            }
            Performance::Low => {
                write!(f, "Low")
            }
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone, PartialEq)]
pub enum Backend {
    // Leave Sugarloaf/WGPU to decide
    #[default]
    #[serde(alias = "automatic")]
    Automatic,
    // Supported on Linux/Android, the web through webassembly via WebGL, and Windows and macOS/iOS via ANGLE
    #[serde(alias = "gl")]
    GL,
    // Supported on Windows, Linux/Android, and macOS/iOS via Vulkan Portability (with the Vulkan feature enabled)
    #[serde(alias = "vulkan")]
    Vulkan,
    // Supported on Windows 10
    #[serde(alias = "dx12")]
    DX12,
    // Supported on macOS/iOS
    #[serde(alias = "metal")]
    Metal,
}

impl Display for Backend {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Backend::Automatic => {
                write!(f, "Automatic")
            }
            Backend::Metal => {
                write!(f, "Metal")
            }
            Backend::Vulkan => {
                write!(f, "Vulkan")
            }
            Backend::GL => {
                write!(f, "GL")
            }
            Backend::DX12 => {
                write!(f, "DX12")
            }
        }
    }
}
