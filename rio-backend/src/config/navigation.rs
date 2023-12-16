use crate::config::colors::{deserialize_to_arr, ColorArray};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    Plain,
    TopTab,
    #[cfg(target_os = "macos")]
    NativeTab,
    BottomTab,
    #[cfg(not(windows))]
    Breadcrumb,
    CollapsedTab,
}

#[allow(clippy::derivable_impls)]
impl Default for NavigationMode {
    fn default() -> NavigationMode {
        #[cfg(target_os = "macos")]
        {
            NavigationMode::NativeTab
        }

        #[cfg(not(target_os = "macos"))]
        {
            NavigationMode::CollapsedTab
        }
    }
}

impl NavigationMode {
    const PLAIN_STR: &'static str = "Plain";
    const COLLAPSED_TAB_STR: &'static str = "CollapsedTab";
    const TOP_TAB_STR: &'static str = "TopTab";
    const BOTTOM_TAB_STR: &'static str = "BottomTab";
    #[cfg(target_os = "macos")]
    const NATIVE_TAB_STR: &'static str = "NativeTab";
    #[cfg(not(windows))]
    const BREADCRUMB_STR: &'static str = "Breadcrumb";

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => Self::PLAIN_STR,
            Self::CollapsedTab => Self::COLLAPSED_TAB_STR,
            Self::TopTab => Self::TOP_TAB_STR,
            Self::BottomTab => Self::BOTTOM_TAB_STR,
            #[cfg(target_os = "macos")]
            Self::NativeTab => Self::NATIVE_TAB_STR,
            #[cfg(not(windows))]
            Self::Breadcrumb => Self::BREADCRUMB_STR,
        }
    }
}

#[inline]
pub fn modes_as_vec_string() -> Vec<String> {
    [
        NavigationMode::Plain,
        NavigationMode::CollapsedTab,
        NavigationMode::TopTab,
        NavigationMode::BottomTab,
        #[cfg(target_os = "macos")]
        NavigationMode::NativeTab,
        #[cfg(not(windows))]
        NavigationMode::Breadcrumb,
    ]
    .iter()
    .map(|navigation_mode| navigation_mode.to_string())
    .collect()
}

impl std::fmt::Display for NavigationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseNavigationModeError;

impl std::str::FromStr for NavigationMode {
    type Err = ParseNavigationModeError;

    fn from_str(s: &str) -> Result<NavigationMode, ParseNavigationModeError> {
        match s {
            Self::COLLAPSED_TAB_STR => Ok(NavigationMode::CollapsedTab),
            Self::TOP_TAB_STR => Ok(NavigationMode::TopTab),
            Self::BOTTOM_TAB_STR => Ok(NavigationMode::BottomTab),
            #[cfg(target_os = "macos")]
            Self::NATIVE_TAB_STR => Ok(NavigationMode::NativeTab),
            #[cfg(not(windows))]
            Self::BREADCRUMB_STR => Ok(NavigationMode::Breadcrumb),
            Self::PLAIN_STR => Ok(NavigationMode::Plain),
            _ => Ok(NavigationMode::default()),
        }
    }
}

#[derive(Default, Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct ColorAutomation {
    #[serde(default = "String::new")]
    pub program: String,
    #[serde(default = "String::new")]
    pub path: String,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "crate::config::colors::defaults::tabs"
    )]
    pub color: ColorArray,
}

#[derive(Debug, Default, PartialEq, Clone, Serialize, Deserialize)]
pub struct Navigation {
    #[serde(default = "NavigationMode::default")]
    pub mode: NavigationMode,
    #[serde(
        default = "Vec::default",
        rename = "color-automation",
        skip_serializing
    )]
    pub color_automation: Vec<ColorAutomation>,
    #[serde(default = "bool::default", skip_serializing)]
    pub clickable: bool,
    #[serde(default = "bool::default", rename = "use-current-path")]
    pub use_current_path: bool,
    #[serde(default = "bool::default", rename = "use-terminal-title")]
    pub use_terminal_title: bool,
}

impl Navigation {
    #[inline]
    pub fn is_collapsed_mode(&self) -> bool {
        self.mode == NavigationMode::CollapsedTab
    }

    #[inline]
    pub fn is_placed_on_bottom(&self) -> bool {
        self.mode == NavigationMode::BottomTab
    }

    #[inline]
    pub fn is_native(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.mode == NavigationMode::NativeTab
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    #[inline]
    pub fn has_navigation_key_bindings(&self) -> bool {
        self.mode != NavigationMode::Plain
    }

    #[inline]
    pub fn is_placed_on_top(&self) -> bool {
        #[cfg(windows)]
        return self.mode == NavigationMode::TopTab;

        #[cfg(not(windows))]
        return self.mode == NavigationMode::TopTab
            || self.mode == NavigationMode::Breadcrumb;
    }
}

#[cfg(test)]
mod tests {
    use crate::config::colors::hex_to_color_arr;
    use crate::config::navigation::{Navigation, NavigationMode};
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
        assert!(decoded.navigation.color_automation.is_empty());
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
        assert!(decoded.navigation.color_automation.is_empty());
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
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_bottom_tab() {
        let content = r#"
            [navigation]
            mode = 'BottomTab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::BottomTab);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.use_current_path);
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_color_automation() {
        let content = r#"
            [navigation]
            mode = 'CollapsedTab'
            color-automation = [
                { program = 'vim', color = '#333333' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::CollapsedTab);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.use_current_path);
        assert!(!decoded.navigation.color_automation.is_empty());
        assert_eq!(
            decoded.navigation.color_automation[0].program,
            "vim".to_string()
        );
        assert_eq!(decoded.navigation.color_automation[0].path, String::new());
        assert_eq!(
            decoded.navigation.color_automation[0].color,
            hex_to_color_arr("#333333")
        );
    }

    #[test]
    fn test_color_automation_arr() {
        let content = r#"
            [navigation]
            mode = 'BottomTab'
            color-automation = [
                { program = 'ssh', color = '#F1F1F1' },
                { program = 'tmux', color = '#333333' },
                { path = '/home', color = '#ffffff' },
                { program = 'nvim', path = '/usr', color = '#00b952' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::BottomTab);
        assert!(!decoded.navigation.clickable);
        assert!(!decoded.navigation.use_current_path);
        assert!(!decoded.navigation.color_automation.is_empty());

        assert_eq!(
            decoded.navigation.color_automation[0].program,
            "ssh".to_string()
        );
        assert_eq!(decoded.navigation.color_automation[0].path, String::new());
        assert_eq!(
            decoded.navigation.color_automation[0].color,
            hex_to_color_arr("#F1F1F1")
        );

        assert_eq!(
            decoded.navigation.color_automation[1].program,
            "tmux".to_string()
        );
        assert_eq!(decoded.navigation.color_automation[1].path, String::new());
        assert_eq!(
            decoded.navigation.color_automation[1].color,
            hex_to_color_arr("#333333")
        );

        assert_eq!(
            decoded.navigation.color_automation[2].program,
            String::new()
        );
        assert_eq!(
            decoded.navigation.color_automation[2].path,
            "/home".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[2].color,
            hex_to_color_arr("#ffffff")
        );

        assert_eq!(
            decoded.navigation.color_automation[3].program,
            "nvim".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[3].path,
            "/usr".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[3].color,
            hex_to_color_arr("#00b952")
        );
    }
}
