use crate::colors::{deserialize_to_arr, ColorArray};
use serde::Deserialize;

#[derive(Default, Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    Plain,
    #[default]
    CollapsedTab,
    TopTab,
    BottomTab,
    #[cfg(target_os = "macos")]
    NativeTab,
    #[cfg(not(windows))]
    Breadcrumb,
}

#[inline]
pub fn modes_as_vec_string() -> Vec<String> {
    vec![
        String::from("Plain"),
        String::from("CollapsedTab"),
        String::from("TopTab"),
        String::from("BottomTab"),
        #[cfg(target_os = "macos")]
        String::from("NativeTab"),
        #[cfg(not(windows))]
        String::from("Breadcrumb"),
    ]
}

impl std::fmt::Display for NavigationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            NavigationMode::CollapsedTab => {
                write!(f, "CollapsedTab")
            }
            NavigationMode::TopTab => {
                write!(f, "TopTab")
            }
            NavigationMode::BottomTab => {
                write!(f, "BottomTab")
            }
            #[cfg(target_os = "macos")]
            NavigationMode::NativeTab => {
                write!(f, "NativeTab")
            }
            #[cfg(not(windows))]
            NavigationMode::Breadcrumb => {
                write!(f, "Breadcrumb")
            }
            NavigationMode::Plain => {
                write!(f, "Plain")
            }
        }
    }
}

#[derive(Default, Debug, Deserialize, PartialEq, Clone)]
pub struct ColorAutomation {
    pub program: String,
    #[serde(
        deserialize_with = "deserialize_to_arr",
        default = "crate::colors::defaults::tabs"
    )]
    pub color: ColorArray,
}

#[derive(Debug, Default, PartialEq, Clone, Deserialize)]
pub struct Navigation {
    #[serde(default = "NavigationMode::default")]
    pub mode: NavigationMode,
    #[serde(default = "Vec::default", rename = "color-automation")]
    pub color_automation: Vec<ColorAutomation>,
    #[serde(default = "bool::default")]
    pub clickable: bool,
    #[serde(default = "bool::default", rename = "use-current-path")]
    pub use_current_path: bool,
    #[serde(default = "bool::default", rename = "use-terminal-title")]
    pub use_terminal_title: bool,
    #[serde(default = "bool::default", rename = "macos-hide-window-buttons")]
    pub macos_hide_window_buttons: bool,
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
    pub fn is_plain(&self) -> bool {
        self.mode == NavigationMode::Plain
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
    use crate::colors::hex_to_color_arr;
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
        assert_eq!(
            decoded.navigation.color_automation[0].color,
            hex_to_color_arr("#F1F1F1")
        );
        assert_eq!(
            decoded.navigation.color_automation[1].program,
            "tmux".to_string()
        );
        assert_eq!(
            decoded.navigation.color_automation[1].color,
            hex_to_color_arr("#333333")
        );
    }
}
