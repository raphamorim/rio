mod defaults;
use crate::defaults::*;
use colors::Colors;
use log::warn;
use serde::Deserialize;
use std::default::Default;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Performance {
    #[default]
    High,
    Low,
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
pub struct Style {
    #[serde(rename = "font-size", default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_theme")]
    pub theme: String,
    #[serde(default = "default_font")]
    pub font: String,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            font_size: default_font_size(),
            theme: default_theme(),
            font: default_font(),
        }
    }
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize)]
pub struct Advanced {
    #[serde(default = "bool::default", rename = "disable-render-when-unfocused")]
    pub disable_render_when_unfocused: bool,
}

#[derive(Debug, PartialEq, Clone, Deserialize)]
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default = "Performance::default")]
    pub performance: Performance,
    #[serde(default = "default_width")]
    pub width: u16,
    #[serde(default = "default_height")]
    pub height: u16,
    #[serde(default = "default_cursor")]
    pub cursor: char,
    #[serde(default = "default_env_vars", rename = "env-vars")]
    pub env_vars: Vec<String>,
    #[serde(default = "default_option_as_alt", rename = "option-as-alt")]
    pub option_as_alt: String,
    #[serde(default = "Style::default")]
    pub style: Style,
    #[serde(default = "Colors::default")]
    pub colors: Colors,
    #[serde(default = "Advanced::default")]
    pub advanced: Advanced,
    #[serde(default = "Developer::default")]
    pub developer: Developer,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Theme {
    #[serde(default = "Colors::default")]
    pub colors: Colors,
}

// Home directory
fn home_dir_path() -> String {
    let base_dir_buffer = dirs::home_dir().unwrap();
    base_dir_buffer.to_str().unwrap_or_default().to_string()
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
                    let theme = &decoded.style.theme;
                    if theme.is_empty() {
                        return Ok(decoded);
                    }

                    let path = format!("/tmp/{theme}.toml");
                    if let Ok(loaded_theme) = Config::load_theme(&path) {
                        decoded.colors = loaded_theme.colors;
                    } else {
                        warn!("failed to load theme: {}", theme);
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

    pub fn load() -> Self {
        let base_dir = home_dir_path();
        let path = format!("{base_dir}/.config/rio/config.toml");
        if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(mut decoded) => {
                    let theme = &decoded.style.theme;
                    if theme.is_empty() {
                        return decoded;
                    }

                    let path = format!("{base_dir}/.config/rio/themes/{theme}.toml");
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
}

impl Default for Config {
    fn default() -> Self {
        Config {
            env_vars: default_env_vars(),
            performance: Performance::default(),
            width: default_width(),
            height: default_height(),
            cursor: default_cursor(),
            option_as_alt: default_option_as_alt(),
            colors: Colors::default(),
            style: Style {
                font_size: default_font_size(),
                theme: default_theme(),
                font: default_font(),
            },
            advanced: Advanced::default(),
            developer: Developer::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colors::{hex_to_color_arr, hex_to_color_wgpu};
    use std::io::Write;

    fn create_temporary_config(prefix: &str, toml_str: &str) -> Config {
        let file_name = format!("/tmp/test-rio-{prefix}-config.toml");
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        match Config::load_from_path_without_fallback(&file_name) {
            Ok(config) => config,
            Err(e) => panic!("{e}"),
        }
    }

    fn create_temporary_theme(theme: &str, toml_str: &str) {
        let file_name = format!("/tmp/{theme}.toml");
        let mut file = std::fs::File::create(file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();
    }

    #[test]
    fn test_filepath_does_not_exist_without_fallback() {
        let should_fail =
            Config::load_from_path_without_fallback("/tmp/it-should-never-exist");
        assert!(should_fail.is_err(), "{}", true);
    }

    #[test]
    fn test_filepath_does_not_exist_with_fallback() {
        let config = Config::load_from_path("/tmp/it-should-never-exist");
        assert_eq!(config.width, default_width());
        assert_eq!(config.height, default_height());
    }

    #[test]
    fn test_empty_config_file() {
        let result = create_temporary_config(
            "empty",
            r#"
            # Config is empty
        "#,
        );

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());

        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());

        // Colors
        assert_eq!(result.colors, Colors::default());

        // Advanced
        assert!(!result.advanced.disable_render_when_unfocused);

        // Developer
        assert_eq!(result.developer.log_level, default_log_level());
        assert!(!result.developer.enable_fps_counter);
    }

    #[test]
    fn test_if_explict_defaults_match() {
        let result = create_temporary_config(
            "defaults",
            r#"
            # Rio default configuration file
            performance = "High"
            height = 438
            width = 662
            cursor = '▇'
            env-vars = []

            [colors]
            background = '#0F0D0E'
            foreground = '#F9F4DA'
            cursor = '#F38BA3'
            tab = '#12B5E5'
            tabs-active = '#FCBA28'
            green = '#0BA95B'
            red = '#ED203D'
            blue = '#12B5E5'
            yellow = '#FCBA28'

            [style]
            font = "CascadiaMono"
            font-size = 16
            theme = ""

            [advanced]
            disable-render-when-unfocused = false

            [developer]
            enable-fps-counter = false
            log-level = "OFF"
        "#,
        );

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.env_vars, default_env_vars());
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.cursor, default_cursor());
        // Style
        assert_eq!(result.style, Style::default());
        // Colors
        assert_eq!(result.colors, Colors::default());
        // Advanced
        assert_eq!(result.advanced, Advanced::default());
        // Developer
        assert_eq!(result.developer, Developer::default());
    }

    #[test]
    fn test_invalid_config_file() {
        let toml_str = r#"
            Performance = 2
            width = "big"
            height = "small"
        "#;

        let file_name = String::from("/tmp/test-rio-invalid-config.toml");
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        let result = Config::load_from_path(&file_name);

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_config_perfomance() {
        let result = create_temporary_config(
            "change-perfomance",
            r#"
            performance = "Low"
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());
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
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.cursor, default_cursor());
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
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
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.cursor, '_');
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());
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
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.option_as_alt, String::from("Both"));
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());
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
        assert_eq!(result.width, 400);
        assert_eq!(result.height, 500);
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, default_theme());
        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
    }

    #[test]
    fn test_change_style() {
        let result = create_temporary_config(
            "change-style",
            r#"
            performance = "Low"

            [style]
            font = "Novamono"
            font-size = 14.0
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        // Style
        assert_eq!(result.style.font, "Novamono");
        assert_eq!(result.style.font_size, 14.0);
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
            [style]
            theme = "lucario"
        "#,
        );

        assert_eq!(result.performance, Performance::High);
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        // Style
        assert_eq!(result.style.font, default_font());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, "lucario");
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
            [style]
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
    }

    #[test]
    fn test_change_advanced() {
        let result = create_temporary_config(
            "change-advanced",
            r#"
            performance = "Low"

            [advanced]
            disable-render-when-unfocused = true
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        // Advanced
        assert!(result.advanced.disable_render_when_unfocused);

        // Colors
        assert_eq!(result.colors.background, colors::defaults::background());
        assert_eq!(result.colors.foreground, colors::defaults::foreground());
        assert_eq!(result.colors.tabs_active, colors::defaults::tabs_active());
        assert_eq!(result.colors.cursor, colors::defaults::cursor());
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
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
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
