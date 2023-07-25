use colors::{deserialize_to_arr, ColorArray};
use serde::Deserialize;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum TabsStyle {
    #[default]
    Collapsed,
    ExpandedTop,
    ExpandedBottom,
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
    color: ColorArray,
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize)]
pub struct Tabs {
    #[serde(default = "TabsStyle::default")]
    pub style: TabsStyle,
    #[serde(default = "Vec::default", rename = "color-rules")]
    pub color_rules: Vec<String>,
    #[serde(default = "bool::default")]
    pub clickable: bool,
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
    fn test_collapsed_tabs() {
        let content = r#"
            [tabs]
            style = 'Collapsed'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.tabs.style, TabsStyle::Collapsed);
        assert!(!decoded.tabs.clickable);
        assert!(decoded.tabs.color_rules.is_empty());
    }

    #[test]
    fn test_expanded_top_tabs() {
        let content = r#"
            [tabs]
            style = 'ExpandedTop'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.tabs.style, TabsStyle::ExpandedTop);
        assert!(!decoded.tabs.clickable);
        assert!(decoded.tabs.color_rules.is_empty());
    }

    #[test]
    fn test_expanded_bottom_tabs() {
        let content = r#"
            [tabs]
            style = 'ExpandedBottom'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.tabs.style, TabsStyle::ExpandedBottom);
        assert!(!decoded.tabs.clickable);
        assert!(decoded.tabs.color_rules.is_empty());
    }
}
