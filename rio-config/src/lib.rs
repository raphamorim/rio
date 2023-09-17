pub mod bindings;
pub mod colors;
pub mod defaults;
pub mod navigation;
pub mod theme;
pub mod window;

use crate::bindings::Bindings;
use crate::defaults::*;
use crate::navigation::Navigation;
use crate::window::Window;
use colors::Colors;
use log::warn;
use serde::{Deserialize, Serialize};
use std::default::Default;
use sugarloaf::font::fonts::SugarloafFonts;
use theme::{AdaptiveColors, AdaptiveTheme, Theme};

#[derive(Clone, Debug)]
pub enum ConfigError {
    ErrLoadingConfig(String),
    ErrLoadingTheme(String),
    PathNotFound,
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum Performance {
    #[default]
    High,
    Low,
}

impl std::fmt::Display for Performance {
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

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Shell {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Developer {
    #[serde(default = "bool::default", rename = "enable-fps-counter")]
    pub enable_fps_counter: bool,
    #[serde(default = "default_log_level", rename = "log-level")]
    pub log_level: String,
}

impl Default for Developer {
    fn default() -> Developer {
        Developer {
            log_level: default_log_level(),
            enable_fps_counter: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "bool::default")]
    pub blinking_cursor: bool,
    #[serde(default = "Navigation::default")]
    pub navigation: Navigation,
    #[serde(default = "Window::default")]
    pub window: Window,
    #[serde(default = "Performance::default")]
    pub performance: Performance,
    #[serde(default = "default_shell")]
    pub shell: Shell,
    #[serde(default = "bool::default", rename = "disable-unfocused-render")]
    pub disable_unfocused_render: bool,
    #[serde(default = "default_use_fork", rename = "use-fork")]
    pub use_fork: bool,
    #[serde(default = "default_working_dir", rename = "working-dir")]
    pub working_dir: Option<String>,
    #[serde(rename = "line-height", default = "default_line_height")]
    pub line_height: f32,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(
        default = "Option::default",
        skip_serializing,
        rename = "adaptive-theme"
    )]
    pub adaptive_theme: Option<AdaptiveTheme>,
    #[serde(default = "SugarloafFonts::default")]
    pub fonts: SugarloafFonts,
    #[serde(rename = "padding-x", default = "default_padding_x")]
    pub padding_x: f32,
    #[serde(default = "default_cursor")]
    pub cursor: char,
    #[serde(default = "default_env_vars", rename = "env-vars")]
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
    #[serde(default = "bool::default")]
    pub ignore_theme_selection_fg_color: bool,
}

#[cfg(not(target_os = "windows"))]
#[inline]
pub fn config_dir_path() -> String {
    let base_dir_buffer = dirs::home_dir().unwrap();
    let home = base_dir_buffer.to_str().unwrap_or_default();
    format!("{home}/.config/rio")
}

#[cfg(target_os = "windows")]
#[inline]
pub fn config_dir_path() -> String {
    let base_dir_buffer = dirs::home_dir().unwrap();
    let home = base_dir_buffer.to_str().unwrap_or_default();
    format!("{home}/AppData/Local/rio")
}

#[inline]
pub fn config_file_path() -> String {
    let config_dir_path_str = config_dir_path();
    format!("{config_dir_path_str}/config.toml")
}

#[inline]
pub fn config_file_content() -> String {
    default_config_file_content()
}

impl Config {
    #[cfg(test)]
    fn load_from_path(path: &str) -> Self {
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            let decoded: Config =
                toml::from_str(&content).unwrap_or_else(|_| Config::default());
            decoded
        } else {
            Config::default()
        }
    }
    #[cfg(test)]
    fn load_from_path_without_fallback(path: &str) -> Result<Self, String> {
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(mut decoded) => {
                    let theme = &decoded.theme;
                    if theme.is_empty() {
                        return Ok(decoded);
                    }

                    let tmp = std::env::temp_dir()
                        .to_str()
                        .unwrap_or_default()
                        .to_string();
                    let path = format!("{tmp}/{theme}.toml");
                    if let Ok(loaded_theme) = Config::load_theme(&path) {
                        decoded.colors = loaded_theme.colors;
                    } else {
                        warn!("failed to load theme: {}", theme);
                    }

                    if let Some(adaptive_theme) = &decoded.adaptive_theme {
                        let light_theme = &adaptive_theme.light;
                        let path = format!("{tmp}/{light_theme}.toml");
                        let mut adaptive_colors = AdaptiveColors {
                            dark: None,
                            light: None,
                        };

                        if let Ok(light_loaded_theme) = Config::load_theme(&path) {
                            adaptive_colors.light = Some(light_loaded_theme.colors);
                            println!("carregou");
                        } else {
                            println!("failed to load light theme: {}", light_theme);
                        }

                        let dark_theme = &adaptive_theme.dark;
                        let path = format!("{tmp}/{dark_theme}.toml");
                        if let Ok(dark_loaded_theme) = Config::load_theme(&path) {
                            adaptive_colors.dark = Some(dark_loaded_theme.colors);
                            println!("carregou");
                        } else {
                            println!("failed to load dark theme: {}", dark_theme);
                        }

                        if adaptive_colors.light.is_some()
                            && adaptive_colors.dark.is_some()
                        {
                            decoded.adaptive_colors = Some(adaptive_colors);
                        }
                    }

                    Ok(decoded)
                }
                Err(err_message) => Err(format!("error parsing: {:?}", err_message)),
            }
        } else {
            Err(String::from("filepath does not exists"))
        }
    }

    fn load_theme(path: &str) -> Result<Theme, String> {
        if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Theme>(&content) {
                Ok(decoded) => Ok(decoded),
                Err(err_message) => Err(format!("error parsing: {:?}", err_message)),
            }
        } else {
            Err(String::from("filepath does not exists"))
        }
    }

    pub fn to_string(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    pub fn load() -> Self {
        let config_path_str = config_dir_path();
        let path = format!("{config_path_str}/config.toml");
        if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(mut decoded) => {
                    let theme = &decoded.theme;
                    if theme.is_empty() {
                        return decoded;
                    }

                    let path = format!("{config_path_str}/themes/{theme}.toml");
                    if let Ok(loaded_theme) = Config::load_theme(&path) {
                        decoded.colors = loaded_theme.colors;
                    } else {
                        warn!("failed to load theme: {}", theme);
                    }

                    decoded
                }
                Err(err_message) => {
                    warn!("failure to parse config file, failling back to default...\n{err_message:?}");
                    Config::default()
                }
            }
        } else {
            Config::default()
        }
    }

    pub fn try_load() -> Result<Self, ConfigError> {
        let config_path_str = config_dir_path();
        let path = format!("{config_path_str}/config.toml");
        if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(mut decoded) => {
                    let theme = &decoded.theme;
                    let theme_path = format!("{config_path_str}/themes");
                    if !theme.is_empty() {
                        let path = format!("{theme_path}/{theme}.toml");
                        match Config::load_theme(&path) {
                            Ok(loaded_theme) => {
                                decoded.colors = loaded_theme.colors;
                            }
                            Err(err_message) => {
                                return Err(ConfigError::ErrLoadingTheme(err_message));
                            }
                        }
                    }

                    if let Some(adaptive_theme) = &decoded.adaptive_theme {
                        let mut adaptive_colors = AdaptiveColors {
                            dark: None,
                            light: None,
                        };

                        let light_theme = &adaptive_theme.light;
                        let path = format!("{theme_path}/{light_theme}.toml");
                        match Config::load_theme(&path) {
                            Ok(light_loaded_theme) => {
                                adaptive_colors.light = Some(light_loaded_theme.colors)
                            }
                            Err(err_message) => {
                                warn!("failed to load light theme: {}", light_theme);
                                return Err(ConfigError::ErrLoadingTheme(err_message));
                            }
                        }

                        let dark_theme = &adaptive_theme.dark;
                        let path = format!("{theme_path}/{dark_theme}.toml");
                        match Config::load_theme(&path) {
                            Ok(dark_loaded_theme) => {
                                adaptive_colors.dark = Some(dark_loaded_theme.colors)
                            }
                            Err(err_message) => {
                                warn!("failed to load dark theme: {}", dark_theme);
                                return Err(ConfigError::ErrLoadingTheme(err_message));
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
            }
        } else {
            Err(ConfigError::PathNotFound)
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            blinking_cursor: false,
            adaptive_theme: None,
            adaptive_colors: None,
            bindings: Bindings::default(),
            colors: Colors::default(),
            cursor: default_cursor(),
            developer: Developer::default(),
            disable_unfocused_render: false,
            env_vars: default_env_vars(),
            fonts: SugarloafFonts::default(),
            line_height: default_line_height(),
            navigation: Navigation::default(),
            option_as_alt: default_option_as_alt(),
            padding_x: default_padding_x(),
            performance: Performance::default(),
            shell: default_shell(),
            theme: default_theme(),
            use_fork: default_use_fork(),
            window: Window::default(),
            working_dir: default_working_dir(),
            ignore_theme_selection_fg_color: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colors::{hex_to_color_arr, hex_to_color_wgpu};
    use std::io::Write;

    fn tmp_dir() -> String {
        std::env::temp_dir()
            .to_str()
            .unwrap_or_default()
            .to_string()
    }

    fn create_temporary_config(prefix: &str, toml_str: &str) -> Config {
        let tmp = tmp_dir();
        let file_name = format!("{tmp}/test-rio-{prefix}-config.toml");
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        match Config::load_from_path_without_fallback(&file_name) {
            Ok(config) => config,
            Err(e) => panic!("{e}"),
        }
    }

    fn create_temporary_theme(theme: &str, toml_str: &str) {
        let tmp = tmp_dir();
        let file_name = format!("{tmp}/{theme}.toml");
        let mut file = std::fs::File::create(file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();
    }

    #[test]
    fn test_filepath_does_not_exist_without_fallback() {
        let tmp = tmp_dir();
        let should_fail = Config::load_from_path_without_fallback(
            format!("{tmp}/it-should-never-exist").as_str(),
        );
        assert!(should_fail.is_err(), "{}", true);
    }

    #[test]
    fn test_filepath_does_not_exist_with_fallback() {
        let tmp = tmp_dir();
        let config =
            Config::load_from_path(format!("{tmp}/it-should-never-exist").as_str());
        assert_eq!(config.theme, default_theme());
        assert_eq!(config.cursor, default_cursor());
    }

    #[test]
    fn test_empty_config_file() {
        let result = create_temporary_config(
            "empty",
            r#"
            # Config is empty
        "#,
        );

        assert!(!result.disable_unfocused_render);

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());

        // Colors
        assert_eq!(result.colors, Colors::default());

        // Developer
        assert_eq!(result.developer.log_level, default_log_level());
        assert!(!result.developer.enable_fps_counter);
    }

    #[test]
    fn test_if_explict_defaults_match() {
        let result = create_temporary_config("defaults", &default_config_file_content());

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.env_vars, default_env_vars());
        assert_eq!(result.window.opacity, default_window_opacity());
        assert_eq!(result.cursor, default_cursor());
        assert_eq!(result.theme, default_theme());
        assert_eq!(result.cursor, default_cursor());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.shell, default_shell());
        assert!(!result.disable_unfocused_render);
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

        let tmp = tmp_dir();
        let file_name =
            String::from(format!("{tmp}/test-rio-invalid-config.toml").as_str());
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        let result = Config::load_from_path(&file_name);

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_performance() {
        let result = create_temporary_config(
            "change-performance",
            r#"
            performance = "Low"
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
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

        assert_eq!(result.performance, Performance::High);
        assert_eq!(result.env_vars, [String::from("A=5"), String::from("B=8")]);
        assert_eq!(result.cursor, default_cursor());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
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
            cursor = '_'
        "#,
        );

        assert_eq!(result.performance, Performance::High);
        assert_eq!(result.cursor, '_');
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
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

        assert_eq!(result.performance, Performance::High);
        assert_eq!(result.option_as_alt, String::from("Both"));
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
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

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
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

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.fonts, SugarloafFonts::default());
        assert_eq!(result.theme, default_theme());
        // Bindings
        assert_eq!(result.bindings.keys[0].key, "Q");
        assert_eq!(result.bindings.keys[0].with, "super");
        assert_eq!(result.bindings.keys[0].action.to_owned(), "Quit");
        assert!(result.bindings.keys[0].text.to_owned().is_empty());
    }

    #[test]
    fn test_change_style() {
        let result = create_temporary_config(
            "change-style",
            r#"
            performance = "Low"
            font-size = 14.0
            line-height = 2.0
            padding-x = 0.0

            [window]
            opacity = 0.5

            [fonts]
            size = 14.0
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.fonts.size, 14.0);
        assert_eq!(result.line_height, 2.0);
        assert_eq!(result.padding_x, 0.0);
        assert_eq!(result.window.opacity, 0.5);
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

        assert_eq!(result.performance, Performance::High);
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
        assert_eq!(result.colors.dim_black, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_blue, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_cyan, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_foreground, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_green, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_magenta, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_red, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_white, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.dim_yellow, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_black, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_blue, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_cyan, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_foreground, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_green, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_magenta, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_red, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_white, hex_to_color_arr("#030303"));
        assert_eq!(result.colors.light_yellow, hex_to_color_arr("#030303"));
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
            performance = "Low"
            disable-unfocused-render = true
            use-fork = true
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        // Advanced
        assert!(result.disable_unfocused_render);
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
    fn test_change_developer() {
        let result = create_temporary_config(
            "change-developer",
            r#"
            performance = "Low"

            [developer]
            enable-fps-counter = true
            log-level = "INFO"
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        // Developer
        assert_eq!(result.developer.log_level, String::from("INFO"));
        assert!(result.developer.enable_fps_counter);

        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }
}
