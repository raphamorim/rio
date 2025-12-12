use serde::{Deserialize, Deserializer, Serialize};

// Panel configuration for split layouts
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Panel {
    #[serde(default = "default_panel_margin")]
    pub margin: Margin,
    #[serde(default = "default_row_gap", rename = "row-gap")]
    pub row_gap: f32,
    #[serde(default = "default_column_gap", rename = "column-gap")]
    pub column_gap: f32,
}

impl Default for Panel {
    fn default() -> Self {
        Self {
            margin: default_panel_margin(),
            row_gap: default_row_gap(),
            column_gap: default_column_gap(),
        }
    }
}

#[inline]
fn default_panel_margin() -> Margin {
    Margin::all(5.0)
}

#[inline]
fn default_row_gap() -> f32 {
    0.0
}

#[inline]
fn default_column_gap() -> f32 {
    0.0
}

// CSS-like margin structure
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Margin {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Margin {
    pub fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    pub fn all(value: f32) -> Self {
        Self::new(value, value, value, value)
    }

    pub fn from_css_values(values: &[f32]) -> Result<Self, String> {
        match values.len() {
            1 => Ok(Self::all(values[0])),
            2 => Ok(Self::new(values[0], values[1], values[0], values[1])),
            4 => Ok(Self::new(values[0], values[1], values[2], values[3])),
            _ => Err(format!(
                "Invalid margin format: expected 1, 2, or 4 values, got {}",
                values.len()
            )),
        }
    }
}

impl Default for Margin {
    fn default() -> Self {
        Self::all(10.0)
    }
}

impl<'de> Deserialize<'de> for Margin {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let values: Vec<f32> = Vec::deserialize(deserializer)?;
        Self::from_css_values(&values).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_margin_all() {
        let margin = Margin::all(10.0);
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 10.0);
        assert_eq!(margin.bottom, 10.0);
        assert_eq!(margin.left, 10.0);
    }

    #[test]
    fn test_margin_from_css_single_value() {
        let margin = Margin::from_css_values(&[10.0]).unwrap();
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 10.0);
        assert_eq!(margin.bottom, 10.0);
        assert_eq!(margin.left, 10.0);
    }

    #[test]
    fn test_margin_from_css_two_values() {
        let margin = Margin::from_css_values(&[10.0, 5.0]).unwrap();
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 5.0);
        assert_eq!(margin.bottom, 10.0);
        assert_eq!(margin.left, 5.0);
    }

    #[test]
    fn test_margin_from_css_four_values() {
        let margin = Margin::from_css_values(&[10.0, 5.0, 15.0, 20.0]).unwrap();
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 5.0);
        assert_eq!(margin.bottom, 15.0);
        assert_eq!(margin.left, 20.0);
    }

    #[test]
    fn test_margin_from_css_invalid_count() {
        let result = Margin::from_css_values(&[10.0, 5.0, 15.0]);
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "Invalid margin format: expected 1, 2, or 4 values, got 3"
        );
    }

    #[test]
    fn test_margin_default() {
        let margin = Margin::default();
        assert_eq!(margin.top, 10.0);
        assert_eq!(margin.right, 10.0);
        assert_eq!(margin.bottom, 10.0);
        assert_eq!(margin.left, 10.0);
    }

    #[test]
    fn test_margin_deserialize_single() {
        let toml_str = r#"margin = [10]"#;
        #[derive(Deserialize)]
        struct Config {
            margin: Margin,
        }
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.margin.top, 10.0);
        assert_eq!(config.margin.right, 10.0);
        assert_eq!(config.margin.bottom, 10.0);
        assert_eq!(config.margin.left, 10.0);
    }

    #[test]
    fn test_margin_deserialize_two() {
        let toml_str = r#"margin = [10, 5]"#;
        #[derive(Deserialize)]
        struct Config {
            margin: Margin,
        }
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.margin.top, 10.0);
        assert_eq!(config.margin.right, 5.0);
        assert_eq!(config.margin.bottom, 10.0);
        assert_eq!(config.margin.left, 5.0);
    }

    #[test]
    fn test_margin_deserialize_four() {
        let toml_str = r#"margin = [10, 5, 15, 20]"#;
        #[derive(Deserialize)]
        struct Config {
            margin: Margin,
        }
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.margin.top, 10.0);
        assert_eq!(config.margin.right, 5.0);
        assert_eq!(config.margin.bottom, 15.0);
        assert_eq!(config.margin.left, 20.0);
    }

    #[test]
    fn test_margin_deserialize_invalid() {
        let toml_str = r#"margin = [10, 5, 15]"#;
        #[derive(Deserialize)]
        struct Config {
            margin: Margin,
        }
        let result: Result<Config, _> = toml::from_str(toml_str);
        assert!(result.is_err());
    }

    // Panel configuration tests
    #[test]
    fn test_panel_default() {
        let panel = Panel::default();
        assert_eq!(panel.margin, Margin::all(5.0));
        assert_eq!(panel.row_gap, 0.0);
        assert_eq!(panel.column_gap, 0.0);
    }

    #[test]
    fn test_panel_deserialize_full() {
        let toml_str = r#"
            [panel]
            margin = [8]
            row-gap = 2
            column-gap = 3
        "#;
        
        #[derive(Deserialize)]
        struct Config {
            panel: Panel,
        }
        
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.panel.margin, Margin::all(8.0));
        assert_eq!(config.panel.row_gap, 2.0);
        assert_eq!(config.panel.column_gap, 3.0);
    }

    #[test]
    fn test_panel_margin_is_inner_spacing() {
        // This test documents that panel.margin is INSIDE the panel
        // It creates space around the terminal content within each panel
        let toml_str = r#"
            [panel]
            margin = [10, 5]  # top/bottom: 10px, left/right: 5px inside panel
            row-gap = 0
            column-gap = 0
        "#;
        
        #[derive(Deserialize)]
        struct Config {
            panel: Panel,
        }
        
        let config: Config = toml::from_str(toml_str).unwrap();
        
        // Panel margin is applied inside each panel
        assert_eq!(config.panel.margin.top, 10.0);
        assert_eq!(config.panel.margin.bottom, 10.0);
        assert_eq!(config.panel.margin.left, 5.0);
        assert_eq!(config.panel.margin.right, 5.0);
        
        // Gaps control spacing BETWEEN panels, not inside
        assert_eq!(config.panel.row_gap, 0.0);
        assert_eq!(config.panel.column_gap, 0.0);
    }

    #[test]
    fn test_panel_with_gaps() {
        let toml_str = r#"
            [panel]
            margin = [5]
            row-gap = 10     # Vertical spacing when split down
            column-gap = 15  # Horizontal spacing when split right
        "#;
        
        #[derive(Deserialize)]
        struct Config {
            panel: Panel,
        }
        
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.panel.margin, Margin::all(5.0));
        assert_eq!(config.panel.row_gap, 10.0);
        assert_eq!(config.panel.column_gap, 15.0);
    }
}
