use serde::Deserialize;
use std::default::Default;
use std::io::Write;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
enum Performance {
    #[default]
    High,
    Average,
    Low,
}

#[derive(Debug, Deserialize, PartialEq)]
struct Config {
    performance: Performance,
    width: u16,
    height: u16,
}

impl Config {
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
    fn load() -> Self {
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

#[allow(dead_code)]
fn create_temporary_config(performance: Performance, width: u16, height: u16) -> Config {
    let toml_str = format!(
        r#"
        # Rio configuration file

        # <performance> Set WGPU rendering performance
        # default: High
        # options: High, Average, Low
        performance = "{:?}"

        # <height> Set default height
        # default: 400
        height = {}

        # <width> Set default width
        # default: 600
        width = {} 

        ## TODO: Add more configs
        "#,
        performance, height, width
    );
    let binding = format!("/tmp/{:?}-config.toml", performance);
    let file_name = binding.as_str();

    let mut file = std::fs::File::create(file_name).unwrap();
    writeln!(file, "{}", toml_str).unwrap(); // writing using the macro 'writeln!'``

    Config::load_from_path(file_name) // load_from_path should just call load() with a custom path
}

impl Default for Config {
    fn default() -> Self {
        Config {
            performance: Performance::default(),
            width: 600,
            height: 400,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_default_config() {
        let expected = Config {
            performance: Performance::High,
            width: 300,
            height: 200,
        };

        let result = create_temporary_config(
            expected.performance,
            expected.width,
            expected.height,
        );
        assert_eq!(result, expected);
    }

    #[test]
    fn load_default_performance_config() {
        let expected = Config {
            performance: Performance::Average,
            width: 400,
            height: 400,
        };

        let result = create_temporary_config(
            expected.performance,
            expected.width,
            expected.height,
        );
        assert_eq!(result, expected);
    }
}
