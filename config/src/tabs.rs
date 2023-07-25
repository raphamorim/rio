use serde::Deserialize;
use colors::{ColorArray, deserialize_to_arr};

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum TabsStyle {
    #[default]
    Minimalist,
    Classic,
    // TODO: Custom comes from plugin system
    // Custom
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct ColorRules {
    key: String,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "colors::defaults::tabs"
    )]
    color: ColorArray
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize)]
pub struct Tabs {
    #[serde(default = "TabsStyle::default")]
    pub style: TabsStyle,
    #[serde(default = "Vec::default", rename = "color-rules")]
    pub color_rules: Vec<String>,
}

#[cfg(test)]
mod tests {

    use crate::tabs::{Tabs, TabsStyle};
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct Root {
        #[serde(default = "Tabs::default")]
        tabs: Tabs,
    }

    #[test]
    fn test_minimalist_tabs() {
        let content = r#"
            [tabs]
            style = 'Minimalist'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.tabs.style, TabsStyle::Minimalist);
        assert!(decoded.tabs.color_rules.is_empty());
    }

    #[test]
    fn test_classic_tabs() {
        let content = r#"
            [tabs]
            style = 'Classic'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.tabs.style, TabsStyle::Classic);
        assert!(decoded.tabs.color_rules.is_empty());
    }

}
