use crate::config::colors::{deserialize_to_arr, deserialize_to_arr_opt, ColorArray};
use crate::config::default_bool_true;
use serde::{Deserialize, Serialize};

#[inline]
pub fn default_unfocused_split_opacity() -> f32 {
    0.7
}

#[inline]
pub fn default_tab_font_size() -> f32 {
    12.0
}

#[inline]
pub fn default_tab_max_width() -> f32 {
    180.0
}

pub fn default_tab_gap() -> f32 {
    6.0
}

pub fn default_tab_inset_y() -> f32 {
    7.0
}

pub fn default_tab_radius() -> f32 {
    6.0
}

pub fn default_tab_bar_height() -> f32 {
    38.0
}

/// Clamp `unfocused_split_opacity` to `[0.15, 1.0]`.
///
/// A value of `0.0` makes the inactive pane invisible, which is never what
/// the user wants; the lower bound keeps the pane legible at the darkest
/// setting.
#[inline]
pub fn clamp_unfocused_split_opacity(v: f32) -> f32 {
    v.clamp(0.15, 1.0)
}

/// Sanitize tab-strip geometry after load: NaN and negative or zero
/// sizes would produce a negative island height, garbage padding, or an
/// oversized hover circle. Fields that may legitimately be `0` (gap,
/// inset, radius, max-width) are only floored at `0`; the two sizes that
/// must stay positive (font size, bar height) get a small lower bound.
impl Navigation {
    #[inline]
    pub fn clamp_tab_geometry(&mut self) {
        let non_negative = |v: f32| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let positive = |v: f32, floor: f32| {
            if v.is_finite() {
                v.max(floor)
            } else {
                floor
            }
        };
        self.tab_font_size = positive(self.tab_font_size, 1.0);
        self.tab_bar_height = positive(self.tab_bar_height, 1.0);
        self.tab_max_width = non_negative(self.tab_max_width);
        self.tab_gap = non_negative(self.tab_gap);
        self.tab_inset_y = non_negative(self.tab_inset_y);
        self.tab_radius = non_negative(self.tab_radius);
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Copy)]
pub enum NavigationMode {
    #[serde(alias = "plain")]
    Plain,
    #[serde(alias = "tab")]
    Tab,
    #[cfg(target_os = "macos")]
    #[serde(alias = "nativetab")]
    NativeTab,
}

#[allow(clippy::derivable_impls)]
impl Default for NavigationMode {
    fn default() -> NavigationMode {
        #[cfg(target_os = "macos")]
        {
            // Use Tab for full GPU rendering
            NavigationMode::Tab
        }

        #[cfg(not(target_os = "macos"))]
        NavigationMode::Tab
    }
}

impl NavigationMode {
    const PLAIN_STR: &'static str = "Plain";
    const TAB_STR: &'static str = "Tab";
    #[cfg(target_os = "macos")]
    const NATIVE_TAB_STR: &'static str = "NativeTab";

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => Self::PLAIN_STR,
            Self::Tab => Self::TAB_STR,
            #[cfg(target_os = "macos")]
            Self::NativeTab => Self::NATIVE_TAB_STR,
        }
    }
}

#[inline]
pub fn modes_as_vec_string() -> Vec<String> {
    [
        NavigationMode::Plain,
        NavigationMode::Tab,
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
            Self::PLAIN_STR => Ok(NavigationMode::Plain),
            Self::TAB_STR => Ok(NavigationMode::Tab),
            #[cfg(target_os = "macos")]
            Self::NATIVE_TAB_STR => Ok(NavigationMode::NativeTab),
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
    /// The opacity level of an unfocused split. A value of `1.0` disables the
    /// dim; lower values fade the pane out. Clamped to `[0.15, 1.0]` at load
    /// time — a value of `0` makes the pane invisible, which is never useful.
    #[serde(
        default = "default_unfocused_split_opacity",
        rename = "unfocused-split-opacity"
    )]
    pub unfocused_split_opacity: f32,
    /// The color used to dim an unfocused split. The overlay's alpha is
    /// derived from `unfocused_split_opacity` — this field is an RGB tint
    /// only. When unset, the terminal's background color is used.
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "unfocused-split-fill"
    )]
    pub unfocused_split_fill: Option<ColorArray>,
    /// Font size (in logical pixels) for the tab-strip titles.
    #[serde(default = "default_tab_font_size", rename = "tab-font-size")]
    pub tab_font_size: f32,
    /// Height (in logical pixels) of the tab strip / island bar.
    #[serde(default = "default_tab_bar_height", rename = "tab-bar-height")]
    pub tab_bar_height: f32,
    /// Maximum width (logical px) of one tab. 0 = no cap: tabs expand
    /// to fill the whole strip.
    #[serde(default = "default_tab_max_width", rename = "tab-max-width")]
    pub tab_max_width: f32,
    /// Horizontal gap (logical px) between tab islands. 0 = tabs touch.
    #[serde(default = "default_tab_gap", rename = "tab-gap")]
    pub tab_gap: f32,
    /// Vertical inset (logical px) of each island inside the strip.
    /// 0 = tabs fill the full bar height (classic flat strip).
    #[serde(default = "default_tab_inset_y", rename = "tab-inset-y")]
    pub tab_inset_y: f32,
    /// Corner radius of each tab island. 0 = square corners.
    #[serde(default = "default_tab_radius", rename = "tab-radius")]
    pub tab_radius: f32,
    /// Explicit island fill for inactive tabs. When unset, fills adapt
    /// to the window background's luminance.
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "tab-fill"
    )]
    pub tab_fill: Option<ColorArray>,
    /// Explicit island fill for the active tab.
    #[serde(
        default = "Option::default",
        deserialize_with = "deserialize_to_arr_opt",
        rename = "tab-fill-active"
    )]
    pub tab_fill_active: Option<ColorArray>,
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
            unfocused_split_fill: None,
            open_config_with_split: true,
            tab_font_size: default_tab_font_size(),
            tab_bar_height: default_tab_bar_height(),
            tab_max_width: default_tab_max_width(),
            tab_gap: default_tab_gap(),
            tab_inset_y: default_tab_inset_y(),
            tab_radius: default_tab_radius(),
            tab_fill: None,
            tab_fill_active: None,
        }
    }
}

impl Navigation {
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
    pub fn is_enabled(&self) -> bool {
        self.mode == NavigationMode::Tab
    }

    /// Whether the rio-rendered tab strip ("island") is actually painted
    /// this frame. Mirrors the gate at `island.rs:358` — input layers
    /// (click routing, cursor override) must agree with the renderer so
    /// the empty band over a hidden island doesn't intercept events.
    #[inline]
    pub fn island_visible(&self, num_tabs: usize) -> bool {
        self.is_enabled() && !(self.hide_if_single && num_tabs == 1)
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
    fn test_plain() {
        let content = r#"
            [navigation]
            mode = 'Plain'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Plain);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_tab() {
        let content = r#"
            [navigation]
            mode = 'Tab'
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Tab);
        assert!(!decoded.navigation.clickable);
        assert!(decoded.navigation.color_automation.is_empty());
    }

    #[test]
    fn test_color_automation() {
        let content = r#"
            [navigation]
            mode = 'Tab'
            color-automation = [
                { program = 'vim', color = '#333333' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Tab);
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
            mode = 'Tab'
            color-automation = [
                { program = 'ssh', color = '#F1F1F1' },
                { program = 'tmux', color = '#333333' },
                { path = '/home', color = '#ffffff' },
                { program = 'nvim', path = '/usr', color = '#00b952' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.navigation.mode, NavigationMode::Tab);
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
