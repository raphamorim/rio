mod defaults;

use crate::defaults::*;
use colors::{deserialize_to_arr, deserialize_to_wgpu, Color, ColorArray};
use serde::Deserialize;
use std::default::Default;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Performance {
    #[default]
    High,
    Low,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Theme {
    Modern,
    #[default]
    Basic,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Font {
    #[default]
    Firamono,
    Novamono,
}

#[derive(Copy, Debug, Deserialize, PartialEq, Clone)]
pub struct Style {
    #[serde(rename = "font-size")]
    pub font_size: f32,
    pub theme: Theme,
    pub font: Font,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            font_size: default_font_size(),
            theme: Theme::default(),
            font: Font::default(),
        }
    }
}

#[derive(Debug, Copy, Deserialize, PartialEq, Clone)]
pub struct Colors {
    #[serde(
        deserialize_with = "deserialize_to_wgpu",
        default = "default_color_background"
    )]
    pub background: Color,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_color_foreground"
    )]
    pub foreground: ColorArray,
    #[serde(
        deserialize_with = "deserialize_to_wgpu",
        default = "default_color_cursor"
    )]
    pub cursor: Color,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "default_color_tabs_active",
        rename = "tabs-active"
    )]
    pub tabs_active: ColorArray,
}

impl Default for Colors {
    fn default() -> Colors {
        Colors {
            background: default_color_background(),
            foreground: default_color_foreground(),
            cursor: default_color_cursor(),
            tabs_active: default_color_tabs_active(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct Advanced {
    #[serde(default = "default_tab_character", rename = "tab-character")]
    pub tab_character: char,
    #[serde(default = "bool::default")]
    pub monochrome: bool,
    #[serde(default = "bool::default", rename = "enable-fps-counter")]
    pub enable_fps_counter: bool,
}

impl Default for Advanced {
    fn default() -> Advanced {
        Advanced {
            tab_character: default_tab_character(),
            monochrome: false,
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
    #[serde(default = "default_columns")]
    pub columns: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default = "Style::default")]
    pub style: Style,
    #[serde(default = "Colors::default")]
    pub colors: Colors,
    #[serde(default = "Advanced::default")]
    pub advanced: Advanced,
}

impl Config {
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    fn load_from_path_without_fallback(path: &str) -> Result<Self, String> {
        if std::path::Path::new(path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str::<Config>(&content) {
                Ok(decoded) => Ok(decoded),
                Err(err_message) => Err(format!("error parsing: {:?}", err_message)),
            }
        } else {
            Err(String::from("filepath does not exists"))
        }
    }

    pub fn load_macos() -> Self {
        // XDG base directory 
        let base_dir_buffer = dirs::config_dir().unwrap();
        let base_dir = base_dir_buffer.to_str().unwrap();

        let path = format!("{base_dir}/.rio/config.toml");
        if std::path::Path::new(&path).exists() {
            let content = std::fs::read_to_string(path).unwrap();
            match toml::from_str(&content) {
                Ok(decoded) => decoded,
                Err(err_message) => {
                    // TODO: Use debug flags
                    println!("failure to parse config file, failling back to default...\n{err_message:?}");
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
            performance: Performance::default(),
            width: default_width(),
            height: default_height(),
            // MacOs default
            columns: default_columns(),
            rows: default_rows(),
            colors: Colors {
                background: default_color_background(),
                foreground: [1.0, 1.0, 1.0, 1.0],
                cursor: default_color_cursor(),
                tabs_active: default_color_tabs_active(),
            },
            style: Style {
                font_size: default_font_size(),
                theme: Theme::default(),
                font: Font::default(),
            },
            advanced: Advanced::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colors::{ColorBuilder, Format};
    use std::io::Write;

    #[allow(dead_code)]
    fn create_temporary_config(prefix: &str, toml_str: &str) -> Config {
        let file_name = format!("/tmp/test-rio-{prefix}-config.toml");
        let mut file = std::fs::File::create(&file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        match Config::load_from_path_without_fallback(&file_name) {
            Ok(config) => config,
            Err(e) => panic!("{e}"),
        }
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
        assert_eq!(config.rows, default_rows());
        assert_eq!(config.columns, default_columns());
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
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Style
        assert_eq!(result.style.font, Font::default());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, Theme::default());
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
        // Advanced
        assert_eq!(result.advanced.tab_character, default_tab_character());
        assert!(!result.advanced.monochrome);
        assert!(!result.advanced.enable_fps_counter);
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

            [colors]
            background = '#151515'
            foreground = '#FFFFFF'
            cursor = '#8E12CC'
            tabs-active = '#F8A145'

            [style]
            font = "Firamono"
            font-size = 16
            theme = "Basic"

            [advanced]
            tab-character = '■'
            monochrome = false
            enable-fps-counter = false
        "#,
        );

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Style
        assert_eq!(result.style.font, Font::default());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, Theme::default());
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
        // Advanced
        assert_eq!(result.advanced.tab_character, default_tab_character());
        assert!(!result.advanced.monochrome);
        assert!(!result.advanced.enable_fps_counter);
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
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Style
        assert_eq!(result.style.font, Font::default());
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, Theme::default());
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
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
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Style
        assert_eq!(result.style.font, Font::Firamono);
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, Theme::Basic);
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
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
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Style
        assert_eq!(result.style.font, Font::Firamono);
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, Theme::Basic);
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
    }

    #[test]
    fn test_change_config_rows_columns() {
        let result = create_temporary_config(
            "change-rows-columns",
            r#"
            rows = 40
            columns = 100
        "#,
        );

        assert_eq!(result.performance, Performance::default());
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.rows, 40);
        assert_eq!(result.columns, 100);
        // Style
        assert_eq!(result.style.font, Font::Firamono);
        assert_eq!(result.style.font_size, default_font_size());
        assert_eq!(result.style.theme, Theme::Basic);
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
    }

    #[test]
    fn test_change_style() {
        let result = create_temporary_config(
            "change-style",
            r#"
            performance = "Low"

            [style]
            font = "Novamono"
            theme = "Modern"
            font-size = 14.0
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Style
        assert_eq!(result.style.font, Font::Novamono);
        assert_eq!(result.style.font_size, 14.0);
        assert_eq!(result.style.theme, Theme::Modern);
        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
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

        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
    }

    #[test]
    fn test_change_colors() {
        let result = create_temporary_config(
            "change-colors",
            r#"
            # Using lucario colors: https://github.com/raphamorim/lucario/

            [colors]
            background = '#2B3E50'
            tabs-active = '#E6DB74'
            foreground = '#F8F8F2'
            cursor = '#E6DB74'
        "#,
        );

        assert_eq!(
            result.colors.background,
            ColorBuilder::from_hex(String::from("#2B3E50"), Format::SRGB0_1)
                .unwrap()
                .to_wgpu()
        );
        assert_eq!(
            result.colors.foreground,
            ColorBuilder::from_hex(String::from("#F8F8F2"), Format::SRGB0_1)
                .unwrap()
                .to_arr()
        );
        assert_eq!(
            result.colors.tabs_active,
            ColorBuilder::from_hex(String::from("#E6DB74"), Format::SRGB0_1)
                .unwrap()
                .to_arr()
        );
        assert_eq!(
            result.colors.cursor,
            ColorBuilder::from_hex(String::from("#E6DB74"), Format::SRGB0_1)
                .unwrap()
                .to_wgpu()
        );
    }

    #[test]
    fn test_change_advanced() {
        let result = create_temporary_config(
            "change-advanced",
            r#"
            performance = "Low"

            [advanced]
            monochrome = true
            enable-fps-counter = true
            tab-character = '▲'            
        "#,
        );

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.width, default_width());
        assert_eq!(result.height, default_height());
        assert_eq!(result.rows, default_rows());
        assert_eq!(result.columns, default_columns());
        // Advanced
        assert!(result.advanced.monochrome);
        assert_eq!(result.advanced.tab_character, '▲');
        assert!(result.advanced.enable_fps_counter);

        // Colors
        assert_eq!(result.colors.background, default_color_background());
        assert_eq!(result.colors.foreground, default_color_foreground());
        assert_eq!(result.colors.tabs_active, default_color_tabs_active());
        assert_eq!(result.colors.cursor, default_color_cursor());
    }
}
