use crate::config::colors::{deserialize_to_arr, ColorArray};
use crate::config::default_bool_true;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    #[serde(alias = "plain")]
    Plain,
    #[serde(alias = "toptab")]
    TopTab,
    #[cfg(target_os = "macos")]
    #[serde(alias = "nativetab")]
    NativeTab,
    #[serde(alias = "bottomtab")]
    BottomTab,
    #[serde(alias = "bookmark")]
    Bookmark,
}

#[allow(clippy::derivable_impls)]
impl Default for NavigationMode {
    fn default() -> NavigationMode {
        #[cfg(target_os = "macos")]
        {
            NavigationMode::NativeTab
        }

        #[cfg(not(target_os = "macos"))]
        NavigationMode::Bookmark
    }
}

impl NavigationMode {
    const PLAIN_STR: &'static str = "Plain";
    const COLLAPSED_TAB_STR: &'static str = "Bookmark";
    const TOP_TAB_STR: &'static str = "TopTab";
    const BOTTOM_TAB_STR: &'static str = "BottomTab";
    #[cfg(target_os = "macos")]
    const NATIVE_TAB_STR: &'static str = "NativeTab";

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => Self::PLAIN_STR,
            Self::Bookmark => Self::COLLAPSED_TAB_STR,
            Self::TopTab => Self::TOP_TAB_STR,
            Self::BottomTab => Self::BOTTOM_TAB_STR,
            #[cfg(target_os = "macos")]
            Self::NativeTab => Self::NATIVE_TAB_STR,
        }
    }
}

#[inline]
pub fn modes_as_vec_string() -> Vec<String> {
    [
        NavigationMode::Plain,
        NavigationMode::Bookmark,
        NavigationMode::TopTab,
        NavigationMode::BottomTab,
        #[cfg(target_os = "macos")]
        NavigationMode::NativeTab,
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
            Self::COLLAPSED_TAB_STR => Ok(NavigationMode::Bookmark),
            Self::TOP_TAB_STR => Ok(NavigationMode::TopTab),
            Self::BOTTOM_TAB_STR => Ok(NavigationMode::BottomTab),
            #[cfg(target_os = "macos")]
            Self::NATIVE_TAB_STR => Ok(NavigationMode::NativeTab),
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

#[inline]
pub fn default_unfocused_split_opacity() -> f32 {
    0.4
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
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
    #[serde(
        default = "default_bool_true",
        rename = "current-working-directory",
        alias = "cwd"
    )]
    pub current_working_directory: bool,
    #[serde(default = "bool::default", rename = "use-terminal-title")]
    pub use_terminal_title: bool,
    #[serde(default = "default_bool_true", rename = "hide-if-single")]
    pub hide_if_single: bool,
    #[serde(default = "default_bool_true", rename = "use-split")]
    pub use_split: bool,
    #[serde(default = "default_bool_true", rename = "open-config-with-split")]
    pub open_config_with_split: bool,
    #[serde(
        default = "default_unfocused_split_opacity",
        rename = "unfocused-split-opacity"
    )]
    pub unfocused_split_opacity: f32,
}

impl Default for Navigation {
    fn default() -> Navigation {
        Navigation {
            mode: NavigationMode::default(),
            color_automation: Vec::default(),
            clickable: false,
            current_working_directory: true,
            use_terminal_title: false,
            hide_if_single: true,
            use_split: true,
            unfocused_split_opacity: default_unfocused_split_opacity(),
            open_config_with_split: true,
        }
    }
}

impl Navigation {
    #[inline]
    pub fn is_collapsed_mode(&self) -> bool {
        self.mode == NavigationMode::Bookmark
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
        self.mode == NavigationMode::TopTab
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
            mode = 'Bookmark'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Bookmark);
        assert!(!decoded.navigation.clickable);
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
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_color_automation() {
        let content = r#"
            [navigation]
            mode = 'Bookmark'
            color-automation = [
                { program = 'vim', color = '#333333' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Bookmark);
        assert!(!decoded.navigation.clickable);
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
