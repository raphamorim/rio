use serde::Deserialize;
use std::default::Default;

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
    fn new(&self) -> Self {
        Self {
            performance: self.performance,
            width: self.width,
            height: self.height,
        }
    }

    fn load() -> Self {
        if std::path::Path::new("/Users/user/.rio/config.toml").exists() {
            let content = std::fs::read_to_string("/Users/user/.rio/config.toml").unwrap();
            // TODO: how can we parse a optional field and set a default for it after?
            // if we let if be Option<T> we will have a None
            // e.g when performance is Option<T>: { inner: ErrorInner  message: "missing field `performance`", key: [] } }
            let decoded: Config = toml::from_str(&content).unwrap();
            println!("eeeee {:#?}", decoded);
            toml::from_str(&content).unwrap()
        } else {
            Config::default()
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new(&Self {
            performance: Performance::default(),
            width: 600,
            height: 400,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    #[test]
    fn load_default_config() {
        let expected = Config {
            performance: Performance::High,
            width: 300,
            height: 200,
        };

        let result = Config::load();
        assert_eq!(result, expected);
    }

    #[test]
    fn load_default_performance_config() {
        let toml_str = r#"
        # Rio configuration file

        # <performance> Set WGPU rendering performance
        # default: High
        # options: High, Average, Low
        performance = "High"

        # <height> Set default height
        # default: 400
        height = 200 

        # <width> Set default width
        # default: 600
        width = 300 

        ## TODO: Add more configs
        "#;

        // TODO: improve this shit
        let mut file = std::fs::File::create("/Users/hungaro/.rio/config.toml").unwrap();
        write!(file, "{}", toml_str).expect("err");

        let expected = Config {
            performance: Performance::High,
            width: 300,
            height: 200,
        };

        let result = Config::load();
        assert_eq!(result, expected);
    }
}
