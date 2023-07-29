use colors::{deserialize_to_arr, ColorArray};
use serde::Deserialize;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    #[default]
    CollapsedTab,
    TopTab,
    BottomTab,
    #[cfg(not(windows))]
    Breadcrumb,
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
    #[serde(default = "bool::default", rename = "use-current-path")]
    pub use_current_path: bool,
    #[serde(default = "bool::default", rename = "use-terminal-title")]
    pub use_terminal_title: bool,
}

impl Navigation {
    pub fn is_collapsed_mode(&self) -> bool {
        self.mode == NavigationMode::CollapsedTab
    }

    pub fn is_placed_on_bottom(&self) -> bool {
        self.mode == NavigationMode::BottomTab
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
    fn test_collapsed_tab() {
        let content = r#"
            [navigation]
            mode = 'CollapsedTab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::CollapsedTab);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_rules.is_empty());
    }

    #[test]
    #[cfg(not(windows))]
    fn test_breadcrumb() {
        let content = r#"
            [navigation]
            mode = 'Breadcrumb'
            use-current-path = true
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Breadcrumb);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.use_current_path);
        assert!(decoded.navigation.color_rules.is_empty());
    }

    #[test]
    fn test_top_tab() {
        let content = r#"
            [navigation]
            mode = 'TopTab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::TopTab);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.use_current_path);
        assert!(decoded.navigation.color_rules.is_empty());
    }

    #[test]
    fn testbottom_tab() {
        let content = r#"
            [navigation]
            mode = 'BottomTab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::BottomTab);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.use_current_path);
        assert!(decoded.navigation.color_rules.is_empty());
    }
}
