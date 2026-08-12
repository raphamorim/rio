use crate::config::colors::{deserialize_to_arr, deserialize_to_arr_opt, ColorArray};
use crate::config::default_bool_true;
use serde::{Deserialize, Serialize};

#[inline]
pub fn default_unfocused_split_opacity() -> f32 {
    0.7
}

/// macOS hides the strip for a lone tab (the native-app feel);
/// Linux/Windows keep it, drawn as a centred title.
#[inline]
pub fn default_hide_if_single() -> bool {
    cfg!(target_os = "macos")
}

#[inline]
pub fn default_max_tab_width() -> f32 {
    240.0
}

/// Clamp `max_tab_width` to `[80.0, 280.0]`.
///
/// Below the lower bound the close button and title no longer fit.
#[inline]
pub fn clamp_max_tab_width(v: f32) -> f32 {
    if !v.is_finite() {
        return default_max_tab_width();
    }
    v.clamp(80.0, 280.0)
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
    #[serde(default = "default_hide_if_single", rename = "hide-if-single")]
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
    /// Maximum width of a tab in logical pixels. Tabs shrink below
    /// this as more open; the cap only limits how wide a tab grows
    /// when few are open. Clamped to `[80.0, 280.0]` at load time.
    #[serde(default = "default_max_tab_width", rename = "max-tab-width")]
    pub max_tab_width: f32,
}

impl Default for Navigation {
    fn default() -> Navigation {
        Navigation {
            mode: NavigationMode::default(),
            color_automation: Vec::default(),
            clickable: false,
            current_working_directory: true,
            use_terminal_title: false,
            hide_if_single: default_hide_if_single(),
            use_split: true,
            unfocused_split_opacity: default_unfocused_split_opacity(),
            unfocused_split_fill: None,
            open_config_with_split: true,
            max_tab_width: default_max_tab_width(),
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

    /// Whether the top band is reserved as custom window chrome this
    /// frame, painted island or not. On macOS the full-size content
    /// view keeps the band whenever Tab navigation is enabled, even
    /// with `hide-if-single` hiding the strip; other platforms render
    /// the terminal from the top when the island is hidden. Must agree
    /// with `padding_top_from_config`, which reserves the band's
    /// height under the same condition.
    #[inline]
    pub fn chrome_band_reserved(&self, num_tabs: usize) -> bool {
        if cfg!(target_os = "macos") {
            self.is_enabled()
        } else {
            self.island_visible(num_tabs)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn chrome_band_follows_padding_contract() {
        use crate::config::navigation::{Navigation, NavigationMode};

        let mut nav = Navigation {
            mode: NavigationMode::Tab,
            hide_if_single: true,
            ..Navigation::default()
        };

        // Visible island: band reserved everywhere.
        assert!(nav.island_visible(2));
        assert!(nav.chrome_band_reserved(2));

        // Hidden island (single tab): macOS keeps the band as chrome,
        // other platforms hand it to the terminal, matching
        // padding_top_from_config.
        assert!(!nav.island_visible(1));
        assert_eq!(nav.chrome_band_reserved(1), cfg!(target_os = "macos"));

        // Non-Tab modes never reserve the band.
        nav.mode = NavigationMode::Plain;
        assert!(!nav.island_visible(1));
        assert!(!nav.chrome_band_reserved(1));
        assert!(!nav.chrome_band_reserved(2));

        #[cfg(target_os = "macos")]
        {
            nav.mode = NavigationMode::NativeTab;
            assert!(!nav.chrome_band_reserved(1));
            assert!(!nav.chrome_band_reserved(2));
        }
    }

    use crate::config::colors::hex_to_color_arr;
    use crate::config::navigation::{Navigation, NavigationMode};
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct Root {
        #[serde(default = "Navigation::default")]
        navigation: Navigation,
    }

    /// The default is platform-split: macOS hides the strip for a lone
    /// tab, Linux/Windows keep it as a centred title with no island
    /// behind it.
    #[test]
    fn hide_if_single_platform_default() {
        let decoded = toml::from_str::<Root>("[navigation]\nmode = 'Tab'\n").unwrap();
        assert_eq!(decoded.navigation.hide_if_single, cfg!(target_os = "macos"));
        assert_eq!(
            decoded.navigation.island_visible(1),
            !cfg!(target_os = "macos")
        );
        assert_eq!(
            Navigation::default().island_visible(1),
            !cfg!(target_os = "macos")
        );
        // More than one tab always shows the strip.
        assert!(decoded.navigation.island_visible(2));
    }

    /// Both explicit values must override the platform default.
    #[test]
    fn hide_if_single_explicit_override() {
        let on =
            toml::from_str::<Root>("[navigation]\nmode = 'Tab'\nhide-if-single = true\n")
                .unwrap();
        assert!(on.navigation.hide_if_single);
        assert!(!on.navigation.island_visible(1));
        assert!(on.navigation.island_visible(2));

        let off = toml::from_str::<Root>(
            "[navigation]\nmode = 'Tab'\nhide-if-single = false\n",
        )
        .unwrap();
        assert!(!off.navigation.hide_if_single);
        assert!(off.navigation.island_visible(1));
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
