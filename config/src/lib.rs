// use colors::Rgba;
use serde::Deserialize;
use std::default::Default;

#[allow(unused_imports)]
use std::io::Write;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Performance {
    #[default]
    High,
    Low,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct Style {
    pub font_size: f32,
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct Colors {
    pub background: String,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Config {
    pub performance: Performance,
    pub width: u16,
    pub height: u16,
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

        let path = format!("{}/.rio/config.toml", base_dir);
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
            colors: Colors {
                background: String::from("#151515"),
            },
            style: Style { font_size: 16.0 },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn create_temporary_config(
        performance: Performance,
        width: u16,
        height: u16,
        style_font_size: f32,
        color_background: String,
    ) -> Config {
        let toml_str = format!(
            r#"
            # Rio configuration file

            # <performance> Set WGPU rendering performance
            # default: High
            # options: High, Low
            performance = "{:?}"

            # <height> Set default height
            # default: 400
            height = {}

            # <width> Set default width
            # default: 600
            width = {}

            [style]
            font_size = {}

            [colors]
            background = {:?}

            ## TODO: Add more configs
            "#,
            performance, height, width, style_font_size, color_background
        );
        let binding = format!("/tmp/{:?}-config.toml", performance);
        let file_name = binding.as_str();

        let mut file = std::fs::File::create(file_name).unwrap();
        writeln!(file, "{}", toml_str).unwrap(); // writing using the macro 'writeln!'``

        Config::load_from_path(file_name) // load_from_path should just call load() with a custom path
    }

    #[test]
    fn load_default_config() {
        let expected = Config {
            performance: Performance::High,
            width: 300,
            height: 200,
            colors: Colors {
                background: String::from("#151515"),
            },
            style: Style { font_size: 18.0 },
        };

        let result = create_temporary_config(
            expected.performance,
            300,
            200,
            18.0,
            String::from("#151515"),
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn load_default_performance_config() {
        let expected = Config {
            performance: Performance::Low,
            width: 400,
            height: 400,
            colors: Colors {
                background: String::from("#151515"),
            },
            style: Style { font_size: 22.0 },
        };

        let result = create_temporary_config(
            expected.performance,
            400,
            400,
            22.0,
            String::from("#151515"),
        );
        assert_eq!(result, expected);
    }
}
