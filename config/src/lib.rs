use colors::Rgba;
use serde::Deserialize;
use std::default::Default;

#[allow(unused_imports)]
use std::io::Write;

/// Default Terminal.App MacOs
pub static COLS_MACOS: u16 = 80;
pub static ROWS_MACOS: u16 = 25;

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

#[derive(Default, Copy, Debug, Deserialize, PartialEq, Clone)]
pub struct Style {
    pub font_size: f32,
    pub theme: Theme,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct Colors {
    #[serde(deserialize_with = "colors::deserialize_hex_string")]
    pub background: Rgba,
    #[serde(deserialize_with = "colors::deserialize_hex_string")]
    pub foreground: Rgba,
    #[serde(deserialize_with = "colors::deserialize_hex_string")]
    pub cursor: Rgba,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Config {
    pub performance: Performance,
    pub width: u16,
    pub height: u16,
    pub columns: u16,
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
            let decoded: Config =
                toml::from_str(&content).unwrap_or_else(|_| Config::default());
            decoded
        } else {
            Config::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            performance: Performance::default(),
            width: 600,
            height: 400,
            // MacOs default
            columns: COLS_MACOS,
            rows: ROWS_MACOS,
            colors: Colors {
                background: Rgba::default(),
                foreground: Rgba {
                    red: 0.255,
                    green: 0.255,
                    blue: 0.255,
                    alpha: 1.0,
                },
                cursor: Rgba {
                    red: 0.142,
                    green: 0.018,
                    blue: 0.204,
                    alpha: 1.0,
                },
            },
            style: Style {
                font_size: 16.0,
                theme: Theme::default(),
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
        style: (f32, Theme),
        colors: (String, String, String),
    ) -> Config {
        let (width, height, columns, rows) = default;
        let (font_size, theme) = style;
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
        let expected = Config {
            performance: Performance::High,
            width: 300,
            height: 200,
            rows: 25,
            columns: 80,
            colors: Colors {
                background: Rgba {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 1.0,
                },
                foreground: Rgba {
                    red: 0.255,
                    green: 0.255,
                    blue: 0.255,
                    alpha: 1.0,
                },
                cursor: Rgba {
                    red: 0.142,
                    green: 0.018,
                    blue: 0.204,
                    alpha: 1.0,
                },
            },
            style: Style {
                theme: Theme::Basic,
                font_size: 18.0,
            },
        };

        let result = create_temporary_config(
            expected.performance,
            (300, 200, 80, 25),
            (18.0, expected.style.theme),
            (
                String::from("#000000"),
                String::from("#FFFFFF"),
                String::from("#8E12CC"),
            ),
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn load_default_performance_config() {
        let expected = Config {
            performance: Performance::Low,
            width: 400,
            height: 400,
            rows: 25,
            columns: 80,
            colors: Colors {
                background: Rgba {
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0,
                    alpha: 1.0,
                },
                foreground: Rgba {
                    red: 0.255,
                    green: 0.255,
                    blue: 0.255,
                    alpha: 1.0,
                },
                cursor: Rgba {
                    red: 0.142,
                    green: 0.018,
                    blue: 0.204,
                    alpha: 1.0,
                },
            },
            style: Style {
                theme: Theme::Basic,
                font_size: 22.0,
            },
        };

        let result = create_temporary_config(
            expected.performance,
            (400, 400, 80, 25),
            (22.0, expected.style.theme),
            (
                String::from("#000000"),
                String::from("#FFFFFF"),
                String::from("#8E12CC"),
            ),
        );
        assert_eq!(result, expected);
    }
}
