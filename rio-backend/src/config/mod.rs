pub mod bell;
pub mod bindings;
pub mod colors;
pub mod defaults;
pub mod hints;
pub mod keyboard;
pub mod navigation;
pub mod platform;
pub mod renderer;
pub mod theme;
pub mod title;
pub mod window;

use crate::ansi::CursorShape;
use crate::config::bell::Bell;
use crate::config::bindings::Bindings;
use crate::config::defaults::*;
use crate::config::hints::Hints;
use crate::config::keyboard::Keyboard;
use crate::config::navigation::Navigation;
use crate::config::platform::{Platform, PlatformConfig};
use crate::config::renderer::Renderer;
use crate::config::title::Title;
use crate::config::window::Window;
use colors::Colors;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::{default::Default, fs::File};
use sugarloaf::font::fonts::SugarloafFonts;
use theme::{AdaptiveColors, AdaptiveTheme, Theme};
use tracing::warn;

#[derive(Clone, Debug)]
pub enum ConfigError {
    ErrLoadingConfig(String),
    ErrLoadingTheme(String),
    PathNotFound,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Shell {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Scroll {
    pub multiplier: f64,
    pub divider: f64,
}

impl Default for Scroll {
    fn default() -> Scroll {
        Scroll {
            multiplier: 3.0,
            divider: 1.0,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Developer {
    #[serde(default = "bool::default", rename = "enable-fps-counter")]
    pub enable_fps_counter: bool,
    #[serde(default = "default_log_level", rename = "log-level")]
    pub log_level: String,
    #[serde(rename = "enable-log-file", default)]
    pub enable_log_file: bool,
}

impl Default for Developer {
    fn default() -> Developer {
        Developer {
            log_level: default_log_level(),
            enable_log_file: false,
            enable_fps_counter: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default)]
    pub cursor: CursorConfig,
    #[serde(default = "Navigation::default")]
    pub navigation: Navigation,
    #[serde(default = "Window::default")]
    pub window: Window,
    #[serde(default = "default_shell")]
    pub shell: Shell,
    #[serde(default = "Platform::default")]
    pub platform: Platform,
    #[serde(default = "default_use_fork", rename = "use-fork")]
    pub use_fork: bool,
    #[serde(default = "Keyboard::default")]
    pub keyboard: Keyboard,
    #[serde(default = "Title::default")]
    pub title: Title,
    #[serde(default = "default_working_dir", rename = "working-dir")]
    pub working_dir: Option<String>,
    #[serde(rename = "line-height", default = "default_line_height")]
    pub line_height: f32,
    #[serde(default = "String::default")]
    pub theme: String,
    #[serde(default = "Scroll::default")]
    pub scroll: Scroll,
    #[serde(
        default = "Option::default",
        skip_serializing,
        rename = "adaptive-theme"
    )]
    pub adaptive_theme: Option<AdaptiveTheme>,
    #[serde(default = "SugarloafFonts::default")]
    pub fonts: SugarloafFonts,
    #[serde(default = "default_editor")]
    pub editor: Shell,
    #[serde(rename = "padding-x", default = "f32::default")]
    pub padding_x: f32,
    #[serde(rename = "padding-y", default = "default_padding_y")]
    pub padding_y: [f32; 2],
    #[serde(default = "Vec::default", rename = "env-vars")]
    pub env_vars: Vec<String>,
    #[serde(default = "default_option_as_alt", rename = "option-as-alt")]
    pub option_as_alt: String,
    #[serde(default = "Colors::default", skip_serializing)]
    pub colors: Colors,
    #[serde(default = "Option::default", skip_serializing)]
    pub adaptive_colors: Option<AdaptiveColors>,
    #[serde(default = "Developer::default")]
    pub developer: Developer,
    #[serde(default = "Bindings::default")]
    pub bindings: bindings::Bindings,
    #[serde(
        default = "bool::default",
        rename = "ignore-selection-foreground-color"
    )]
    pub ignore_selection_fg_color: bool,
    #[serde(default = "default_bool_true", rename = "confirm-before-quit")]
    pub confirm_before_quit: bool,
    #[serde(
        default = "bool::default",
        rename = "hide-mouse-cursor-when-typing",
        alias = "hide-cursor-when-typing"
    )]
    pub hide_cursor_when_typing: bool,
    #[serde(default = "Renderer::default")]
    pub renderer: Renderer,
    #[serde(default = "bool::default", rename = "draw-bold-text-with-light-colors")]
    pub draw_bold_text_with_light_colors: bool,
    #[serde(default = "Hints::default")]
    pub hints: Hints,
    #[serde(default = "Bell::default")]
    pub bell: Bell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CursorConfig {
    #[serde(default = "default_cursor")]
    pub shape: CursorShape,
    #[serde(default = "bool::default")]
    pub blinking: bool,
    #[serde(default = "default_cursor_interval", rename = "blinking-interval")]
    pub blinking_interval: u64,
}

#[cfg(target_os = "macos")]
#[inline]
pub fn config_dir_path() -> PathBuf {
    std::env::var("RIO_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(dirs::home_dir().unwrap().join(".config").join("rio"))
}

#[cfg(target_os = "windows")]
#[inline]
pub fn config_dir_path() -> PathBuf {
    std::env::var("RIO_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(
            dirs::home_dir()
                .unwrap()
                .join("AppData")
                .join("Local")
                .join("rio"),
        )
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[inline]
pub fn config_dir_path() -> PathBuf {
    std::env::var("RIO_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or(dirs::home_dir().unwrap().join(".config"))
                .join("rio"),
        )
}

#[inline]
pub fn config_file_path() -> PathBuf {
    config_dir_path().join("config.toml")
}

#[inline]
pub fn config_file_content() -> String {
    default_config_file_content()
}

#[inline]
pub fn create_config_file(path: Option<PathBuf>) {
    let default_file_path = path.clone().unwrap_or(config_file_path());
    if default_file_path.exists() {
        tracing::info!(
            "configuration file already exists at {}",
            default_file_path.display()
        );
        return;
    }

    if path.is_none() {
        let default_dir_path = config_dir_path();
        match std::fs::create_dir_all(&default_dir_path) {
            Ok(_) => {
                tracing::info!(
                    "configuration path created {}",
                    default_dir_path.display()
                );
            }
            Err(err_message) => {
                tracing::error!("could not create config directory: {err_message}");
            }
        }
    }

    match File::create(&default_file_path) {
        Err(err_message) => {
            tracing::error!(
                "could not create config file {}: {err_message}",
                default_file_path.display()
            )
        }
        Ok(mut created_file) => {
            tracing::info!("configuration file created {}", default_file_path.display());

            if let Err(err_message) = writeln!(created_file, "{}", config_file_content())
            {
                tracing::error!(
                    "could not update config file with defaults: {err_message}"
                )
            }
        }
    }
}

impl Config {
    #[cfg(test)]
    fn load_from_path(path: &PathBuf) -> Self {
        if path.exists() {
            let content = std::fs::read_to_string(path).unwrap();
            let decoded: Config =
                toml::from_str(&content).unwrap_or_else(|_| Config::default());
            decoded
        } else {
            Config::default()
        }
    }
    #[cfg(test)]
    fn load_from_path_without_fallback(path: &PathBuf) -> Result<Self, String> {
        if path.exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(mut decoded) => {
                    let theme = &decoded.theme;
                    if theme.is_empty() {
                        return Ok(decoded);
                    }

                    let tmp = std::env::temp_dir();
                    let path = tmp.join(theme).with_extension("toml");
                    if let Ok(loaded_theme) = Config::load_theme(&path) {
                        decoded.colors = loaded_theme.colors;
                    } else {
                        warn!("failed to load theme: {}", theme);
                    }

                    if let Some(adaptive_theme) = &decoded.adaptive_theme {
                        let light_theme = &adaptive_theme.light;
                        let path = tmp.join(light_theme).with_extension("toml");
                        let mut adaptive_colors = AdaptiveColors {
                            dark: None,
                            light: None,
                        };

                        if let Ok(light_loaded_theme) = Config::load_theme(&path) {
                            adaptive_colors.light = Some(light_loaded_theme.colors);
                        } else {
                            warn!("failed to load light theme: {}", light_theme);
                        }

                        let dark_theme = &adaptive_theme.dark;
                        let path = tmp.join(dark_theme).with_extension("toml");
                        if let Ok(dark_loaded_theme) = Config::load_theme(&path) {
                            adaptive_colors.dark = Some(dark_loaded_theme.colors);
                        } else {
                            warn!("failed to load dark theme: {}", dark_theme);
                        }

                        if adaptive_colors.light.is_some()
                            && adaptive_colors.dark.is_some()
                        {
                            decoded.adaptive_colors = Some(adaptive_colors);
                        }
                    }

                    Ok(decoded)
                }
                Err(err_message) => Err(format!("error parsing: {err_message:?}")),
            }
        } else {
            Err(String::from("filepath does not exist"))
        }
    }

    fn load_theme(path: &PathBuf) -> Result<Theme, String> {
        if path.exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Theme>(&content) {
                Ok(decoded) => Ok(decoded),
                Err(err_message) => Err(format!("error parsing: {err_message:?}")),
            }
        } else {
            Err(String::from("filepath does not exist"))
        }
    }

    pub fn to_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    pub fn load() -> Self {
        let config_path = config_dir_path();
        let path = config_file_path();
        if path.exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(mut decoded) => {
                    let theme = &decoded.theme;
                    if theme.is_empty() {
                        return decoded;
                    }

                    let path = config_path
                        .join("themes")
                        .join(theme)
                        .with_extension("toml");
                    if let Ok(loaded_theme) = Config::load_theme(&path) {
                        decoded.colors = loaded_theme.colors;
                    } else {
                        warn!("failed to load theme: {}", theme);
                    }

                    decoded
                }
                Err(err_message) => {
                    warn!("failure to parse config file, falling back to default...\n{err_message:?}");
                    Config::default()
                }
            }
        } else {
            Config::default()
        }
    }

    pub fn try_load() -> Result<Self, ConfigError> {
        let path = config_file_path();
        if path.exists() {
            match std::fs::read_to_string(path) {
                Ok(content) => match toml::from_str::<Config>(&content) {
                    Ok(mut decoded) => {
                        let theme = &decoded.theme;
                        let theme_path = config_dir_path().join("themes");
                        if !theme.is_empty() {
                            let path = theme_path.join(theme).with_extension("toml");
                            match Config::load_theme(&path) {
                                Ok(loaded_theme) => {
                                    decoded.colors = loaded_theme.colors;
                                }
                                Err(err_message) => {
                                    return Err(ConfigError::ErrLoadingTheme(
                                        err_message,
                                    ));
                                }
                            }
                        }

                        if let Some(adaptive_theme) = &decoded.adaptive_theme {
                            let mut adaptive_colors = AdaptiveColors {
                                dark: None,
                                light: None,
                            };

                            let light_theme = &adaptive_theme.light;
                            let path =
                                theme_path.join(light_theme).with_extension("toml");
                            match Config::load_theme(&path) {
                                Ok(light_loaded_theme) => {
                                    adaptive_colors.light =
                                        Some(light_loaded_theme.colors)
                                }
                                Err(err_message) => {
                                    warn!("failed to load light theme: {}", light_theme);
                                    return Err(ConfigError::ErrLoadingTheme(
                                        err_message,
                                    ));
                                }
                            }

                            let dark_theme = &adaptive_theme.dark;
                            let path = theme_path.join(dark_theme).with_extension("toml");
                            match Config::load_theme(&path) {
                                Ok(dark_loaded_theme) => {
                                    adaptive_colors.dark = Some(dark_loaded_theme.colors)
                                }
                                Err(err_message) => {
                                    warn!("failed to load dark theme: {}", dark_theme);
                                    return Err(ConfigError::ErrLoadingTheme(
                                        err_message,
                                    ));
                                }
                            }

                            if adaptive_colors.light.is_some()
                                && adaptive_colors.dark.is_some()
                            {
                                decoded.adaptive_colors = Some(adaptive_colors);
                            }
                        }

                        Ok(decoded)
                    }
                    Err(err_message) => {
                        Err(ConfigError::ErrLoadingConfig(err_message.to_string()))
                    }
                },
                Err(err_message) => {
                    Err(ConfigError::ErrLoadingConfig(err_message.to_string()))
                }
            }
        } else {
            Err(ConfigError::PathNotFound)
        }
    }

    pub fn overwrite_based_on_platform(&mut self) {
        #[cfg(windows)]
        if let Some(windows) = &self.platform.windows {
            self.overwrite_with_platform_config(windows.clone());
        }

        #[cfg(target_os = "linux")]
        if let Some(linux) = &self.platform.linux {
            self.overwrite_with_platform_config(linux.clone());
        }

        #[cfg(target_os = "macos")]
        if let Some(macos) = &self.platform.macos {
            self.overwrite_with_platform_config(macos.clone());
        }
    }

    fn overwrite_with_platform_config(&mut self, platform_config: PlatformConfig) {
        // Replace shell entirely if specified
        if let Some(shell_overwrite) = &platform_config.shell {
            self.shell = shell_overwrite.clone();
        }

        // Merge window fields individually
        if let Some(window_overwrite) = &platform_config.window {
            if let Some(width) = window_overwrite.width {
                self.window.width = width;
            }
            if let Some(height) = window_overwrite.height {
                self.window.height = height;
            }
            if let Some(mode) = window_overwrite.mode {
                self.window.mode = mode;
            }
            if let Some(opacity) = window_overwrite.opacity {
                self.window.opacity = opacity;
            }
            if let Some(blur) = window_overwrite.blur {
                self.window.blur = blur;
            }
            if let Some(bg_image) = &window_overwrite.background_image {
                self.window.background_image = Some(bg_image.clone());
            }
            if let Some(decorations) = window_overwrite.decorations {
                self.window.decorations = decorations;
            }
            if let Some(macos_unified) = window_overwrite.macos_use_unified_titlebar {
                self.window.macos_use_unified_titlebar = macos_unified;
            }
            if let Some(macos_shadow) = window_overwrite.macos_use_shadow {
                self.window.macos_use_shadow = macos_shadow;
            }
            if let Some(initial_title) = &window_overwrite.initial_title {
                self.window.initial_title = Some(initial_title.clone());
            }
            if let Some(win_shadow) = window_overwrite.windows_use_undecorated_shadow {
                self.window.windows_use_undecorated_shadow = Some(win_shadow);
            }
            if let Some(win_bitmap) = window_overwrite.windows_use_no_redirection_bitmap {
                self.window.windows_use_no_redirection_bitmap = Some(win_bitmap);
            }
            if let Some(win_corner) = &window_overwrite.windows_corner_preference {
                self.window.windows_corner_preference = Some(win_corner.clone());
            }
            if let Some(colorspace) = window_overwrite.colorspace {
                self.window.colorspace = colorspace;
            }
        }

        // Merge navigation fields individually
        if let Some(navigation_overwrite) = &platform_config.navigation {
            if let Some(mode) = navigation_overwrite.mode {
                self.navigation.mode = mode;
            }
            if let Some(color_automation) = &navigation_overwrite.color_automation {
                self.navigation.color_automation = color_automation.clone();
            }
            if let Some(clickable) = navigation_overwrite.clickable {
                self.navigation.clickable = clickable;
            }
            if let Some(cwd) = navigation_overwrite.current_working_directory {
                self.navigation.current_working_directory = cwd;
            }
            if let Some(use_term_title) = navigation_overwrite.use_terminal_title {
                self.navigation.use_terminal_title = use_term_title;
            }
            if let Some(hide_if_single) = navigation_overwrite.hide_if_single {
                self.navigation.hide_if_single = hide_if_single;
            }
            if let Some(use_split) = navigation_overwrite.use_split {
                self.navigation.use_split = use_split;
            }
            if let Some(open_cfg_split) = navigation_overwrite.open_config_with_split {
                self.navigation.open_config_with_split = open_cfg_split;
            }
            if let Some(unfocused_opacity) = navigation_overwrite.unfocused_split_opacity
            {
                self.navigation.unfocused_split_opacity = unfocused_opacity;
            }
        }

        // Merge renderer fields individually
        if let Some(renderer_overwrite) = &platform_config.renderer {
            if let Some(performance) = renderer_overwrite.performance {
                self.renderer.performance = performance;
            }
            if let Some(backend) = &renderer_overwrite.backend {
                self.renderer.backend = backend.clone();
            }
            if let Some(disable_unfocused) = renderer_overwrite.disable_unfocused_render {
                self.renderer.disable_unfocused_render = disable_unfocused;
            }
            if let Some(disable_occluded) = renderer_overwrite.disable_occluded_render {
                self.renderer.disable_occluded_render = disable_occluded;
            }
            if let Some(filters) = &renderer_overwrite.filters {
                self.renderer.filters = filters.clone();
            }
            if let Some(strategy) = &renderer_overwrite.strategy {
                self.renderer.strategy = strategy.clone();
            }
        }

        // Append platform-specific env vars to the global ones
        if let Some(env_vars_overwrite) = &platform_config.env_vars {
            self.env_vars.extend(env_vars_overwrite.clone());
        }

        // Override theme
        if let Some(theme_overwrite) = &platform_config.theme {
            self.theme = theme_overwrite.clone();
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            cursor: CursorConfig::default(),
            editor: default_editor(),
            adaptive_theme: None,
            adaptive_colors: None,
            bindings: Bindings::default(),
            colors: Colors::default(),
            scroll: Scroll::default(),
            keyboard: Keyboard::default(),
            title: Title::default(),
            developer: Developer::default(),
            env_vars: vec![],
            fonts: SugarloafFonts::default(),
            line_height: default_line_height(),
            navigation: Navigation::default(),
            option_as_alt: default_option_as_alt(),
            padding_x: f32::default(),
            padding_y: default_padding_y(),
            renderer: Renderer::default(),
            shell: default_shell(),
            platform: Platform::default(),
            theme: String::default(),
            use_fork: default_use_fork(),
            window: Window::default(),
            working_dir: default_working_dir(),
            ignore_selection_fg_color: false,
            confirm_before_quit: true,
            hide_cursor_when_typing: false,
            draw_bold_text_with_light_colors: false,
            hints: Hints::default(),
            bell: Bell::default(),
        }
    }
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: default_cursor(),
            blinking: false,
            blinking_interval: default_cursor_interval(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colors::{hex_to_color_arr, hex_to_color_wgpu};
    use std::io::Write;
    use sugarloaf::font::fonts::parse_unicode;

    fn tmp_dir() -> PathBuf {
        std::env::temp_dir()
    }

    fn create_temporary_config(prefix: &str, toml_str: &str) -> Config {
        let file_name = tmp_dir().join(format!("test-rio-{prefix}-config.toml"));
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        match Config::load_from_path_without_fallback(&file_name) {
            Ok(config) => config,
            Err(e) => panic!("{e}"),
        }
    }

    fn create_temporary_theme(theme: &str, toml_str: &str) {
        let file_name = tmp_dir().join(theme).with_extension("toml");
        let mut file = std::fs::File::create(file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();
    }

    #[test]
    fn test_filepath_does_not_exist_without_fallback() {
        let should_fail = Config::load_from_path_without_fallback(
            &tmp_dir().join("it-should-never-exist"),
        );
        assert!(should_fail.is_err(), "{}", true);
    }

    #[test]
    fn test_filepath_does_not_exist_with_fallback() {
        let config = Config::load_from_path(&tmp_dir().join("it-should-never-exist"));
        assert_eq!(config.theme, String::default());
        assert_eq!(config.cursor.shape, default_cursor());
    }

    #[test]
    fn test_empty_config_file() {
        let result = create_temporary_config(
            "empty",
            r#"
            # Config is empty
        "#,
        );

        assert!(!result.renderer.disable_unfocused_render);

        assert_eq!(
            result.renderer.performance,
            renderer::Performance::default()
        );
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());

        // Colors
        assert_eq!(result.colors, Colors::default());

        // Developer
        assert_eq!(result.developer.log_level, default_log_level());
        assert!(!result.developer.enable_fps_counter);
    }

    #[test]
    fn test_if_explicit_defaults_match() {
        let result = create_temporary_config("defaults", &default_config_file_content());

        assert_eq!(
            result.renderer.performance,
            renderer::Performance::default()
        );
        let env_vars: Vec<String> = vec![];
        assert_eq!(result.env_vars, env_vars);
        assert_eq!(result.cursor.shape, default_cursor());
        assert_eq!(result.theme, String::default());
        assert_eq!(result.cursor.shape, default_cursor());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.shell, default_shell());
        assert!(!result.renderer.disable_unfocused_render);
        assert_eq!(result.use_fork, default_use_fork());
        assert_eq!(result.line_height, default_line_height());

        // Colors
        assert_eq!(result.colors, Colors::default());
        // Developer
        assert_eq!(result.developer, Developer::default());
        assert_eq!(result.bindings, Bindings::default());
    }

    #[test]
    fn test_invalid_config_file() {
        let toml_str = r#"
            Performance = 2
            width = "big"
            height = "small"
        "#;

        let file_name = tmp_dir()
            .join("test-rio-invalid-config")
            .with_extension("toml");
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        let result = Config::load_from_path(&file_name);

        assert_eq!(
            result.renderer.performance,
            renderer::Performance::default()
        );
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_renderer() {
        let result = create_temporary_config(
            "change-performance",
            r#"
            [renderer]
            performance = "Low"
            backend = "Vulkan"
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::Low);
        assert_eq!(result.renderer.backend, renderer::Backend::Vulkan);
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_renderer_occlusion() {
        let result = create_temporary_config(
            "change-renderer-occlusion",
            r#"
            [renderer]
            disable-occluded-render = false
        "#,
        );

        assert!(!result.renderer.disable_occluded_render);
        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_environment_variables() {
        let result = create_temporary_config(
            "change-env-vars",
            r#"
            env-vars = ['A=5', 'B=8']
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert_eq!(result.env_vars, [String::from("A=5"), String::from("B=8")]);
        assert_eq!(result.cursor.shape, default_cursor());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(
            result.colors.selection_background,
            colors::defaults::selection_background()
        );
        assert_eq!(
            result.colors.selection_foreground,
            colors::defaults::selection_foreground()
        );
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_cursor() {
        let result = create_temporary_config(
            "change-cursor",
            r#"
            [cursor]
            shape = 'underline'
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert_eq!(result.renderer.backend, renderer::Backend::Automatic);
        assert_eq!(result.cursor.shape, CursorShape::Underline);
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_option_as_alt() {
        let result = create_temporary_config(
            "change-option-as-alt",
            r#"
            option-as-alt = 'Both'
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert_eq!(result.option_as_alt, String::from("Both"));
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_width_height() {
        let result = create_temporary_config(
            "change-width-height",
            r#"
            width = 400
            height = 500
        "#,
        );

        assert_eq!(
            result.renderer.performance,
            renderer::Performance::default()
        );
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_bindings() {
        let result = create_temporary_config(
            "change-key-bindings",
            r#"
            [bindings]
            keys = [
                { key = 'Q', with = 'super', action = 'Quit' }
            ]
        "#,
        );

        assert_eq!(
            result.renderer.performance,
            renderer::Performance::default()
        );
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, String::default());
        // Bindings
        assert_eq!(result.bindings.keys[0].key, "Q");
        assert_eq!(result.bindings.keys[0].with, "super");
        assert_eq!(result.bindings.keys[0].action.to_owned(), "Quit");
        assert!(result.bindings.keys[0].esc.to_owned().is_empty());
    }

    #[test]
    fn test_change_style() {
        let result = create_temporary_config(
            "change-style",
            r#"
            font-size = 14.0
            line-height = 2.0
            padding-x = 0.0

            [renderer]
            performance = "Low"

            [window]
            opacity = 0.5
            [window.background-image]
            path = "my-image-path.png"

            [fonts]
            size = 14.0
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::Low);
        assert_eq!(result.fonts.size, 14.0);
        assert_eq!(result.line_height, 2.0);
        assert_eq!(result.padding_x, 0.0);
        assert_eq!(result.window.opacity, 0.5);
        assert_eq!(
            result.window.background_image,
            Some(sugarloaf::ImageProperties {
                path: String::from("my-image-path.png"),
                ..sugarloaf::ImageProperties::default()
            })
        );
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_theme() {
        let result = create_temporary_config(
            "change-theme",
            r#"
            theme = "lucario"
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, "lucario");
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_theme_with_colors_overwrite() {
        create_temporary_theme(
            "lucario-with-colors",
            r#"
            [colors]
            background       = '#2B3E50'
            foreground       = '#F8F8F2'
        "#,
        );

        let result = create_temporary_config(
            "change-theme-with-colors",
            r#"
            theme = "lucario-with-colors"

            [colors]
            background = '#333333'
            foreground = '#333333'
        "#,
        );

        // Colors
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
        assert_eq!(result.colors.foreground, hex_to_color_arr("#F8F8F2"));
        assert_eq!(result.colors.background.0, hex_to_color_arr("#2B3E50"));
    }

    #[test]
    fn test_change_one_color() {
        let result = create_temporary_config(
            "change-one-color",
            r#"
            [colors]
            foreground = '#000000'
        "#,
        );

        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_colors() {
        let result = create_temporary_config(
            "change-colors",
            r#"
            [colors]
            background       = '#2B3E50'
            tabs-active      = '#E6DB74'
            selection-background = '#111111'
            selection-foreground = '#222222'
            foreground       = '#F8F8F2'
            cursor           = '#E6DB74'
            black            = '#FFFFFF'
            blue             = '#030303'
            cyan             = '#030303'
            green            = '#030303'
            magenta          = '#030303'
            red              = '#030303'
            tabs             = '#030303'
            white            = '#000000'
            yellow           = '#030303'
            dim-black        = '#030303'
            dim-blue         = '#030303'
            dim-cyan         = '#030303'
            dim-foreground   = '#030303'
            dim-green        = '#030303'
            dim-magenta      = '#030303'
            dim-red          = '#030303'
            dim-white        = '#030303'
            dim-yellow       = '#030303'
            light-black      = '#030303'
            light-blue       = '#030303'
            light-cyan       = '#030303'
            light-foreground = '#030303'
            light-green      = '#030303'
            light-magenta    = '#030303'
            light-red        = '#030303'
            light-white      = '#030303'
            light-yellow     = '#030303'
        "#,
        );

        // assert_eq!(
        //     result.colors.background,
        //     ColorBuilder::from_hex(String::from("#2B3E50"), Format::SRGB0_1)
        //         .unwrap()
        //         .to_wgpu()
        // );

        assert_eq!(result.colors.background.0, hex_to_color_arr("#2B3E50"));
        assert_eq!(result.colors.background.1, hex_to_color_wgpu("#2B3E50"));
        assert_eq!(result.colors.cursor, hex_to_color_arr("#E6DB74"));
        assert_eq!(result.colors.foreground, hex_to_color_arr("#F8F8F2"));
        assert_eq!(result.colors.tabs_active, hex_to_color_arr("#E6DB74"));
        assert_eq!(result.colors.black, hex_to_color_arr("#FFFFFF"));
        assert_eq!(result.colors.blue, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.cyan, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.green, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.magenta, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.red, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.tabs, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.white, hex_to_color_arr("#000000"));
        assert_eq!(result.colors.yellow, hex_to_color_arr("#030303"));
        assert_eq!(
            result.colors.selection_background,
            hex_to_color_arr("#111111")
        );
        assert_eq!(
            result.colors.selection_foreground,
            hex_to_color_arr("#222222")
        );
    }

    #[test]
    fn test_use_fork() {
        let result = create_temporary_config(
            "change-use-fork",
            r#"
            use-fork = true

            [renderer]
            disable-unfocused-render = true
            performance = "Low"
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::Low);
        // Advanced
        assert!(result.renderer.disable_unfocused_render);
        assert!(result.use_fork);

        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_shell() {
        let result = create_temporary_config(
            "change-shell-and-editor",
            r#"
            shell = { program = "/bin/fish", args = ["--hello"] }
        "#,
        );

        assert_eq!(result.shell.program, "/bin/fish");
        assert_eq!(result.shell.args, ["--hello"]);
    }

    #[test]
    fn test_shell_no_args() {
        let result = create_temporary_config(
            "change-shell-and-editor-no-args",
            r#"
            shell = { program = "/bin/fish" }
        "#,
        );

        assert_eq!(result.shell.program, "/bin/fish");
        assert_eq!(result.shell.args, Vec::<&str>::new());
    }

    #[test]
    fn test_change_developer_and_performance() {
        let result = create_temporary_config(
            "change-developer",
            r#"
            [renderer]
            performance = "Low"
            backend = "GL"

            [developer]
            enable-fps-counter = true
            log-level = "INFO"
        "#,
        );

        assert_eq!(result.renderer.performance, renderer::Performance::Low);
        assert_eq!(result.renderer.backend, renderer::Backend::GL);
        // Developer
        assert_eq!(result.developer.log_level, String::from("INFO"));
        assert!(result.developer.enable_fps_counter);

        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_symbol_map() {
        let result = create_temporary_config(
            "symbol-map",
            r#"
            fonts.symbol-map = [
                # covers: '⊗','⊘','⊙'
                { start = "2297", end = "2299", font-family = "PowerlineSymbols" },
                { start = "E0C0", end = "E0C7", font-family = "Cascadia Code NF" },
            ]
        "#,
        );

        assert!(result.fonts.symbol_map.is_some());
        let symbol_map = result.fonts.symbol_map.unwrap();
        assert_eq!(symbol_map.len(), 2);
        assert_eq!(symbol_map[0].font_family, "PowerlineSymbols");
        assert_eq!(symbol_map[0].start, "2297");
        assert_eq!(symbol_map[0].end, "2299");

        assert_eq!(parse_unicode(&symbol_map[0].start), Some('\u{2297}'));
        assert_eq!(parse_unicode(&symbol_map[0].end), Some('\u{2299}'));

        assert_eq!(symbol_map[1].font_family, "Cascadia Code NF");
        assert_eq!(symbol_map[1].start, "E0C0");
        assert_eq!(symbol_map[1].end, "E0C7");

        assert_eq!(parse_unicode(&symbol_map[1].start), Some('\u{E0C0}'));
        assert_eq!(parse_unicode(&symbol_map[1].end), Some('\u{E0C7}'));
    }

    #[test]
    fn test_window_colorspace() {
        let result = create_temporary_config(
            "window-colorspace",
            r#"
            [window]
            colorspace = "display-p3"
        "#,
        );

        assert_eq!(result.window.colorspace, window::Colorspace::DisplayP3);
    }

    #[test]
    fn test_window_colorspace_default() {
        let result = create_temporary_config(
            "window-colorspace-default",
            r#"
            [window]
            width = 800
            height = 600
        "#,
        );

        #[cfg(target_os = "macos")]
        assert_eq!(result.window.colorspace, window::Colorspace::DisplayP3);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(result.window.colorspace, window::Colorspace::Srgb);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_specific_env_vars() {
        let mut result = create_temporary_config(
            "platform-env-vars",
            r#"
            env-vars = ["GLOBAL=value", "FOO=bar"]

            [platform]
            macos.env-vars = ["MACOS_ONLY=yes", "PLATFORM_VAR=macos"]
        "#,
        );

        // Apply platform overrides
        result.overwrite_based_on_platform();

        // Should have both global and platform-specific env vars
        assert_eq!(result.env_vars.len(), 4);
        assert!(result.env_vars.contains(&String::from("GLOBAL=value")));
        assert!(result.env_vars.contains(&String::from("FOO=bar")));
        assert!(result.env_vars.contains(&String::from("MACOS_ONLY=yes")));
        assert!(result
            .env_vars
            .contains(&String::from("PLATFORM_VAR=macos")));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn test_platform_specific_env_vars_linux() {
        let mut result = create_temporary_config(
            "platform-env-vars-linux",
            r#"
            env-vars = ["GLOBAL=value"]

            [platform]
            linux.env-vars = ["LINUX_ONLY=yes"]
        "#,
        );

        result.overwrite_based_on_platform();

        assert_eq!(result.env_vars.len(), 2);
        assert!(result.env_vars.contains(&String::from("GLOBAL=value")));
        assert!(result.env_vars.contains(&String::from("LINUX_ONLY=yes")));
    }

    #[test]
    #[cfg(windows)]
    fn test_platform_specific_env_vars_windows() {
        let mut result = create_temporary_config(
            "platform-env-vars-windows",
            r#"
            env-vars = ["GLOBAL=value"]

            [platform]
            windows.env-vars = ["WINDOWS_ONLY=yes"]
        "#,
        );

        result.overwrite_based_on_platform();

        assert_eq!(result.env_vars.len(), 2);
        assert!(result.env_vars.contains(&String::from("GLOBAL=value")));
        assert!(result.env_vars.contains(&String::from("WINDOWS_ONLY=yes")));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_window_field_level_merge() {
        let mut result = create_temporary_config(
            "platform-window-merge",
            r#"
            [window]
            width = 800
            height = 600
            opacity = 0.75
            blur = true

            [platform]
            macos.window.mode = "Maximized"
        "#,
        );

        result.overwrite_based_on_platform();

        // Mode should be overridden
        assert_eq!(result.window.mode, window::WindowMode::Maximized);
        // But other fields should be preserved
        assert_eq!(result.window.width, 800);
        assert_eq!(result.window.height, 600);
        assert_eq!(result.window.opacity, 0.75);
        assert!(result.window.blur);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_shell_replace() {
        let mut result = create_temporary_config(
            "platform-shell-replace",
            r#"
            shell = { program = "/bin/bash", args = ["--login"] }

            [platform]
            macos.shell = { program = "/bin/zsh", args = ["-l"] }
        "#,
        );

        result.overwrite_based_on_platform();

        // Shell should be completely replaced
        assert_eq!(result.shell.program, "/bin/zsh");
        assert_eq!(result.shell.args, vec!["-l"]);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_renderer_merge() {
        let mut result = create_temporary_config(
            "platform-renderer-merge",
            r#"
            [renderer]
            performance = "High"
            disable-unfocused-render = true

            [platform]
            macos.renderer.backend = "Metal"
        "#,
        );

        result.overwrite_based_on_platform();

        // Backend should be set
        assert_eq!(result.renderer.backend, renderer::Backend::Metal);
        // Other fields should be preserved
        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert!(result.renderer.disable_unfocused_render);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_navigation_merge() {
        let mut result = create_temporary_config(
            "platform-navigation-merge",
            r#"
            [navigation]
            mode = "TopTab"
            clickable = true

            [platform]
            macos.navigation.mode = "NativeTab"
        "#,
        );

        result.overwrite_based_on_platform();

        // Mode should be overridden
        assert_eq!(
            result.navigation.mode,
            navigation::NavigationMode::NativeTab
        );
        // Clickable should be preserved
        assert!(result.navigation.clickable);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_theme_override() {
        let mut result = create_temporary_config(
            "platform-theme-override",
            r#"
            theme = "default-theme"

            [platform]
            macos.theme = "macos-specific-theme"
        "#,
        );

        result.overwrite_based_on_platform();

        // Theme should be overridden
        assert_eq!(result.theme, "macos-specific-theme");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn test_platform_complex_merge() {
        let mut result = create_temporary_config(
            "platform-complex-merge",
            r#"
            env-vars = ["GLOBAL=1"]
            theme = "default"

            [window]
            width = 1024
            height = 768
            opacity = 0.9
            blur = false

            [renderer]
            performance = "Low"
            disable-unfocused-render = false

            [navigation]
            mode = "BottomTab"
            clickable = false

            shell = { program = "/bin/sh", args = ["-c"] }

            [platform]
            macos.env-vars = ["MACOS=1"]
            macos.theme = "macos-theme"
            macos.window.opacity = 1.0
            macos.window.blur = true
            macos.renderer.performance = "High"
            macos.navigation.clickable = true
            macos.shell = { program = "/bin/zsh", args = ["--login"] }
        "#,
        );

        result.overwrite_based_on_platform();

        // Env vars should be merged
        assert!(result.env_vars.contains(&String::from("GLOBAL=1")));
        assert!(result.env_vars.contains(&String::from("MACOS=1")));

        // Theme overridden
        assert_eq!(result.theme, "macos-theme");

        // Window: opacity and blur overridden, others preserved
        assert_eq!(result.window.opacity, 1.0);
        assert!(result.window.blur);
        assert_eq!(result.window.width, 1024);
        assert_eq!(result.window.height, 768);

        // Renderer: performance overridden, disable_unfocused_render preserved
        assert_eq!(result.renderer.performance, renderer::Performance::High);
        assert!(!result.renderer.disable_unfocused_render);

        // Navigation: clickable overridden, mode preserved
        assert!(result.navigation.clickable);
        assert_eq!(
            result.navigation.mode,
            navigation::NavigationMode::BottomTab
        );

        // Shell: completely replaced
        assert_eq!(result.shell.program, "/bin/zsh");
        assert_eq!(result.shell.args, vec!["--login"]);
    }

    #[test]
    fn test_multiple_platform_configs_dont_interfere() {
        let result = create_temporary_config(
            "multi-platform",
            r#"
            env-vars = ["GLOBAL=1"]

            [platform]
            linux.env-vars = ["LINUX=1"]
            windows.env-vars = ["WINDOWS=1"]
            macos.env-vars = ["MACOS=1"]
        "#,
        );

        // Before applying platform overrides, should only have global env vars
        assert_eq!(result.env_vars.len(), 1);
        assert!(result.env_vars.contains(&String::from("GLOBAL=1")));
    }
}
