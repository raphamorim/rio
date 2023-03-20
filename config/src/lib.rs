use colors::{Color, ColorBuilder, Format};
use serde::Deserialize;
use std::default::Default;

#[allow(unused_imports)]
use std::io::Write;

/// Default Terminal.App MacOs
pub static COLS_MACOS: u16 = 80;
pub static ROWS_MACOS: u16 = 25;

fn default_cols() -> u16 {
    COLS_MACOS
}

fn default_rows() -> u16 {
    ROWS_MACOS
}

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
    pub font_size: f32,
    pub theme: Theme,
    pub font: Font,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct Colors {
    #[serde(deserialize_with = "colors::deserialize_hex_string")]
    pub background: Color,
    #[serde(
        deserialize_with = "colors::deserialize_hex_string",
        default = "Color::default"
    )]
    pub foreground: Color,
    #[serde(
        deserialize_with = "colors::deserialize_hex_string",
        default = "Color::default"
    )]
    pub cursor: Color,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Config {
    pub performance: Performance,
    pub width: u16,
    pub height: u16,
    #[serde(default = "default_cols")]
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
                    println!("{err_message:?}");
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
        let foreground = ColorBuilder::from_hex(String::from("#FFFFFF"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let cursor = ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        Config {
            performance: Performance::default(),
            width: 662,
            height: 438,
            // MacOs default
            columns: COLS_MACOS,
            rows: ROWS_MACOS,
            colors: Colors {
                background,
                foreground,
                cursor,
            },
            style: Style {
                font_size: 16.0,
                theme: Theme::default(),
                font: Font::default(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn create_temporary_config(
        performance: Performance,
        default: (u16, u16, u16, u16),
        style: (f32, Theme, Font),
        colors: (String, String, String),
    ) -> Config {
        let (width, height, columns, rows) = default;
        let (font_size, theme, font) = style;
        let (background, foreground, cursor) = colors;

        let toml_str = format!(
            r#"
            # Rio configuration file

            # <performance> Set WGPU rendering performance
            # default: High
            # options: High, Low
            performance = "{performance:?}"

            # <height> Set default height
            # default: 400
            height = {height}

            # <width> Set default width
            # default: 600
            width = {width}

            columns = {columns}
            rows = {rows}

            [style]
            font_size = {font_size}
            font = "{font:?}"
            theme = "{theme:?}"

            [colors]
            background = {background:?}
            foreground = {foreground:?}
            cursor = {cursor:?}

            ## TODO: Add more configs
            "#
        );
        let binding = format!("/tmp/{performance:?}-config.toml");
        let file_name = binding.as_str();

        let mut file = std::fs::File::create(file_name).unwrap();
        writeln!(file, "{toml_str}").unwrap();

        Config::load_from_path(file_name)
    }

    #[test]
    fn load_default_config() {
        let background = ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let foreground = ColorBuilder::from_hex(String::from("#FFFFFF"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let cursor = ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();

        let expected = Config {
            performance: Performance::High,
            width: 300,
            height: 200,
            rows: 25,
            columns: 80,
            colors: Colors {
                background,
                foreground,
                cursor,
            },
            style: Style {
                theme: Theme::Basic,
                font_size: 18.0,
                font: Font::Firamono,
            },
        };

        let result = create_temporary_config(
            expected.performance,
            (300, 200, 80, 25),
            (18.0, expected.style.theme, expected.style.font),
            (
                String::from("#151515"),
                String::from("#FFFFFF"),
                String::from("#8E12CC"),
            ),
        );
        assert_eq!(expected.performance, result.performance);
        assert_eq!(expected.colors.background, result.colors.background);
        assert_eq!(expected.colors.foreground, result.colors.foreground);
        assert_eq!(expected.colors.cursor, result.colors.cursor);
        assert_eq!(expected.style.font, result.style.font);
        assert_eq!(expected.style.theme, result.style.theme);
        assert_eq!(expected.width, result.width);
        assert_eq!(expected.rows, result.rows);
        assert_eq!(expected.columns, result.columns);
    }

    #[test]
    fn load_default_performance_config() {
        let background = ColorBuilder::from_hex(String::from("#000000"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let foreground = ColorBuilder::from_hex(String::from("#FFFFFF"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();
        let cursor = ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
            .unwrap()
            .to_wgpu();

        let expected = Config {
            performance: Performance::Low,
            width: 400,
            height: 400,
            rows: 25,
            columns: 80,
            colors: Colors {
                background,
                foreground,
                cursor,
            },
            style: Style {
                theme: Theme::Basic,
                font_size: 22.0,
                font: Font::Novamono,
            },
        };

        let result = create_temporary_config(
            expected.performance,
            (400, 400, 80, 25),
            (22.0, expected.style.theme, expected.style.font),
            (
                String::from("#000000"),
                String::from("#FFFFFF"),
                String::from("#8E12CC"),
            ),
        );

        assert_eq!(expected.performance, result.performance);
        assert_eq!(expected.colors.background, result.colors.background);
        assert_eq!(expected.colors.foreground, result.colors.foreground);
        assert_eq!(expected.colors.cursor, result.colors.cursor);
        assert_eq!(expected.style.font, result.style.font);
        assert_eq!(expected.style.theme, result.style.theme);
        assert_eq!(expected.width, result.width);
        assert_eq!(expected.rows, result.rows);
        assert_eq!(expected.columns, result.columns);
    }
}
