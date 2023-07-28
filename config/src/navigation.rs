use colors::{deserialize_to_arr, ColorArray};
use serde::Deserialize;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    #[default]
    CollapsedTabs,
    Breadcrumb,
    TopTabs,
    BottomTabs,
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
pub struct Navigation {
    #[serde(default = "NavigationMode::default")]
    pub mode: NavigationMode,
    #[serde(default = "Vec::default", rename = "color-rules")]
    pub color_rules: Vec<String>,
    #[serde(default = "bool::default")]
    pub clickable: bool,
}

impl Navigation {
    pub fn is_collapsed_mode(&self) -> bool {
        self.mode == NavigationMode::CollapsedTabs
    }

    pub fn is_placed_on_bottom(&self) -> bool {
        self.mode == NavigationMode::BottomTabs
    }
}

#[cfg(test)]
mod tests {

    use crate::navigation::{Navigation, NavigationMode};
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct Root {
        #[serde(default = "Navigation::default")]
        navigation: Navigation,
    }

    #[test]
    fn test_collapsed_tabs() {
        let content = r#"
            [navigation]
            mode = 'CollapsedTabs'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::CollapsedTabs);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_rules.is_empty());
    }

    #[test]
    fn test_breadcrumb() {
        let content = r#"
            [navigation]
            mode = 'Breadcrumb'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Breadcrumb);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_rules.is_empty());
    }

    #[test]
    fn test_top_tabs() {
        let content = r#"
            [navigation]
            mode = 'TopTabs'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::TopTabs);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_rules.is_empty());
    }

    #[test]
    fn testbottom_tabs() {
        let content = r#"
            [navigation]
            mode = 'BottomTabs'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::BottomTabs);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_rules.is_empty());
    }
}
