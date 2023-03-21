mod defaults;

use crate::defaults::*;
use colors::{Color, ColorArray, ColorBuilder, Format};
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

#[derive(Default, Copy, Debug, Deserialize, PartialEq, Clone)]
pub struct Style {
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    pub theme: Theme,
    pub font: Font,
}

#[derive(Default, Debug, Copy, Deserialize, PartialEq, Clone)]
pub struct Colors {
    #[serde(deserialize_with = "colors::deserialize_to_wpgu")]
    pub background: Color,
    #[serde(deserialize_with = "colors::deserialize_to_arr")]
    pub foreground: ColorArray,
    #[serde(deserialize_with = "colors::deserialize_to_wpgu")]
    pub cursor: Color,
    #[serde(deserialize_with = "colors::deserialize_to_arr")]
    pub tabs_active: ColorArray,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Config {
    pub performance: Performance,
    #[serde(default = "default_width")]
    pub width: u16,
    #[serde(default = "default_height")]
    pub height: u16,
    #[serde(default = "default_columns")]
    pub columns: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    pub style: Style,
    pub colors: Colors,
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
            match toml::from_str(&content) {
                Ok(decoded) => decoded,
                Err(err_message) => Err(format!("error parsing: {:?}", err_message)),
            }
        } else {
            Err(String::from("filepath does not exists"))
        }
    }

    pub fn load_macos() -> Self {
        let base_dir_buffer = dirs::home_dir().unwrap();
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
        let background = ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let cursor = ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let tabs_active =
            ColorBuilder::from_hex(String::from("#F8A145"), Format::SRGB0_1)
                .unwrap()
                .to_arr();
        Config {
            performance: Performance::default(),
            width: default_width(),
            height: default_height(),
            // MacOs default
            columns: default_columns(),
            rows: default_rows(),
            colors: Colors {
                background,
                foreground: [1.0, 1.0, 1.0, 1.0],
                cursor,
                tabs_active,
            },
            style: Style {
                font_size: default_font_size(),
                theme: Theme::default(),
                font: Font::default(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[allow(dead_code)]
    fn create_temporary_config(toml_str: &str) -> Config {
        let file_name = String::from("/tmp/test-rio-config.toml");
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
    fn test_single_config_change_and_keep_defaults() {
        let result =
            create_temporary_config(String::from("performance = \"Low\"\n").as_str());

        // background = "#151515"
        // foreground = "#FFFFFF"
        // cursor = "#8E12CC"
        // tabs_active = "#F8A145"

        assert_eq!(result.performance, Performance::Low);
        assert_eq!(result.style.font, Font::Firamono);
        assert_eq!(result.style.theme, Theme::Basic);
        assert_eq!(result.width, 400);
        assert_eq!(result.height, 500);
        assert_eq!(result.rows, 25);
        assert_eq!(result.columns, 80);

        let expected_background =
            ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_1)
                .unwrap()
                .to_wgpu();
        let expected_foreground = [1.0, 1.0, 1.0, 1.0];
        let expected_tabs_active =
            ColorBuilder::from_hex(String::from("#F8A145"), Format::SRGB0_1)
                .unwrap()
                .to_arr();
        let expected_cursor =
            ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
                .unwrap()
                .to_wgpu();

        assert_eq!(result.colors.background, expected_background);
        assert_eq!(result.colors.foreground, expected_foreground);
        assert_eq!(result.colors.tabs_active, expected_tabs_active);
        assert_eq!(result.colors.cursor, expected_cursor);
    }

    // #[test]
    // fn test_changing_all_values() {
    //     let background = ColorBuilder::from_hex(String::from("#000000"), Format::SRGB0_1)
    //         .unwrap()
    //         .to_wgpu();
    //     let cursor = ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
    //         .unwrap()
    //         .to_wgpu();
    //     let tabs_active =
    //         ColorBuilder::from_hex(String::from("#E6DB74"), Format::SRGB0_1)
    //             .unwrap()
    //             .to_arr();

    //     let expected = Config {
    //         performance: Performance::Low,
    //         width: 400,
    //         height: 400,
    //         rows: 25,
    //         columns: 80,
    //         colors: Colors {
    //             background,
    //             foreground: [1.0, 1.0, 1.0, 1.0],
    //             cursor,
    //             tabs_active,
    //         },
    //         style: Style {
    //             theme: Theme::Basic,
    //             font_size: 22.0,
    //             font: Font::Novamono,
    //         },
    //     };

    //     let result = create_temporary_config(
    //         expected.performance,
    //         (400, 400, 80, 25),
    //         (22.0, expected.style.theme, expected.style.font),
    //         (
    //             String::from("#000000"),
    //             String::from("#FFFFFF"),
    //             String::from("#8E12CC"),
    //             String::from("#E6DB74"),
    //         ),
    //     );

    //     assert_eq!(expected.performance, result.performance);
    //     assert_eq!(expected.colors.background, result.colors.background);
    //     assert_eq!(expected.colors.foreground, result.colors.foreground);
    //     assert_eq!(expected.colors.cursor, result.colors.cursor);
    //     assert_eq!(expected.style.font, result.style.font);
    //     assert_eq!(expected.style.theme, result.style.theme);
    //     assert_eq!(expected.width, result.width);
    //     assert_eq!(expected.rows, result.rows);
    //     assert_eq!(expected.columns, result.columns);
    // }
}
