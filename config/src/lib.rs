use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
enum Performances {
    High,
    Average,
    Low,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Config {
    performance: Performances,
    width: Option<u16>,
    height: Option<u16>,
}

pub fn load() -> Config {
    let toml_str = r#"
    # Rio configuration file

    # <perfomance> Set WGPU rendering perfomance
    # default: High
    # options: High, Average, Low
    performance = "High"

    # <height> Set default height
    # default: 400
    height = 400

    # <width> Set default width
    # default: 600
    width = 600

    ## TODO: Add more configs
    "#;

    let decoded: Config = toml::from_str(toml_str).unwrap();

    decoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_config() {
        let expected = Config {
            performance: Performances::High,
            width: Some(600),
            height: Some(400),
        };

        let result = load();
        assert_eq!(result, expected);
    }
}
