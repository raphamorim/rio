use crate::config::navigation;
use crate::config::renderer;
use crate::config::window;
use crate::config::Shell;
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Platform {
    pub linux: Option<PlatformConfig>,
    pub windows: Option<PlatformConfig>,
    pub macos: Option<PlatformConfig>,
}

/// Other platform specific configuration options can be added here.
///
/// When deserializing, each field is Option<T> to distinguish between
/// "not specified" vs "specified with value". During merge, we recursively
/// merge individual fields rather than replacing entire structures.
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PlatformConfig {
    #[serde(default = "Option::default")]
    pub shell: Option<Shell>,
    #[serde(default = "Option::default")]
    pub navigation: Option<PlatformNavigation>,
    #[serde(default = "Option::default")]
    pub window: Option<PlatformWindow>,
    #[serde(default = "Option::default")]
    pub renderer: Option<PlatformRenderer>,
    #[serde(default = "Option::default", rename = "env-vars")]
    pub env_vars: Option<Vec<String>>,
    #[serde(default = "Option::default")]
    pub theme: Option<String>,
}

/// Platform-specific window config with optional fields for selective override
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PlatformWindow {
    #[serde(default = "Option::default")]
    pub width: Option<i32>,
    #[serde(default = "Option::default")]
    pub height: Option<i32>,
    #[serde(default = "Option::default")]
    pub mode: Option<window::WindowMode>,
    #[serde(default = "Option::default")]
    pub opacity: Option<f32>,
    #[serde(default = "Option::default")]
    pub blur: Option<bool>,
    #[serde(
        default = "Option::default",
        rename = "background-image",
        skip_serializing
    )]
    pub background_image: Option<sugarloaf::ImageProperties>,
    #[serde(default = "Option::default")]
    pub decorations: Option<window::Decorations>,
    #[serde(default = "Option::default", rename = "macos-use-unified-titlebar")]
    pub macos_use_unified_titlebar: Option<bool>,
    #[serde(default = "Option::default", rename = "macos-use-shadow")]
    pub macos_use_shadow: Option<bool>,
    #[serde(default = "Option::default", rename = "initial-title")]
    pub initial_title: Option<String>,
    #[serde(default = "Option::default", rename = "windows-use-undecorated-shadow")]
    pub windows_use_undecorated_shadow: Option<bool>,
    #[serde(
        default = "Option::default",
        rename = "windows-use-no-redirection-bitmap"
    )]
    pub windows_use_no_redirection_bitmap: Option<bool>,
    #[serde(default = "Option::default", rename = "windows-corner-preference")]
    pub windows_corner_preference: Option<window::WindowsCornerPreference>,
    #[serde(default = "Option::default")]
    pub colorspace: Option<window::Colorspace>,
}

/// Platform-specific navigation config with optional fields for selective override
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PlatformNavigation {
    #[serde(default = "Option::default")]
    pub mode: Option<navigation::NavigationMode>,
    #[serde(default = "Option::default", rename = "color-automation")]
    pub color_automation: Option<Vec<navigation::ColorAutomation>>,
    #[serde(default = "Option::default")]
    pub clickable: Option<bool>,
    #[serde(default = "Option::default", rename = "current-working-directory")]
    pub current_working_directory: Option<bool>,
    #[serde(default = "Option::default", rename = "use-terminal-title")]
    pub use_terminal_title: Option<bool>,
    #[serde(default = "Option::default", rename = "hide-if-single")]
    pub hide_if_single: Option<bool>,
    #[serde(default = "Option::default", rename = "use-split")]
    pub use_split: Option<bool>,
    #[serde(default = "Option::default", rename = "open-config-with-split")]
    pub open_config_with_split: Option<bool>,
    #[serde(default = "Option::default", rename = "unfocused-split-opacity")]
    pub unfocused_split_opacity: Option<f32>,
}

/// Platform-specific renderer config with optional fields for selective override
#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct PlatformRenderer {
    #[serde(default = "Option::default")]
    pub performance: Option<renderer::Performance>,
    #[serde(default = "Option::default")]
    pub backend: Option<renderer::Backend>,
    #[serde(default = "Option::default", rename = "disable-unfocused-render")]
    pub disable_unfocused_render: Option<bool>,
    #[serde(default = "Option::default", rename = "disable-occluded-render")]
    pub disable_occluded_render: Option<bool>,
    #[serde(default = "Option::default", skip_serializing)]
    pub filters: Option<Vec<sugarloaf::Filter>>,
    #[serde(default = "Option::default")]
    pub strategy: Option<renderer::RendererStategy>,
}
