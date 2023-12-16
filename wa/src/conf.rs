// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT (https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT) and slightly modified

/// Platform specific settings.
#[derive(Debug)]
pub struct Platform {
    /// On some platform it is possible to ask the OS for a specific swap interval.
    /// Note that this is highly platform and implementation dependent,
    /// there is no guarantee that FPS will be equal to swap_interval.
    /// In other words - "swap_interval" is a hint for a GPU driver, this is not
    /// the way to limit FPS in the game!
    pub swap_interval: Option<i32>,

    /// Whether the framebuffer should have an alpha channel.
    /// Currently supported only on Android
    /// TODO: Document(and check) what does it actually mean on android. Transparent window?
    pub framebuffer_alpha: bool,

    /// Whether to draw the default window decorations on Wayland.
    /// Only works when using the Wayland backend.
    pub wayland_use_fallback_decorations: bool,
}

impl Default for Platform {
    fn default() -> Platform {
        Platform {
            swap_interval: None,
            framebuffer_alpha: false,
            wayland_use_fallback_decorations: true,
        }
    }
}

#[derive(Debug)]
pub struct Conf {
    /// Title of the window, defaults to an empty string.
    pub window_title: String,
    /// The preferred width of the window, ignored on wasm/android.
    ///
    /// Default: 800
    pub window_width: i32,
    /// The preferred height of the window, ignored on wasm/android.
    ///
    /// Default: 600
    pub window_height: i32,
    /// Whether the rendering canvas is full-resolution on HighDPI displays.
    ///
    /// Default: false
    pub high_dpi: bool,
    /// Whether the window should be created in fullscreen mode, ignored on wasm/android.
    ///
    /// Default: false
    pub fullscreen: bool,
    /// MSAA sample count
    ///
    /// Default: 1
    pub sample_count: i32,

    /// Determines if the application user can resize the window
    pub window_resizable: bool,

    /// Miniquad allows to change the window icon programmatically.
    /// The icon will be used as
    /// - TODO: dock and titlebar icon on  MacOs
    pub icon: Option<Icon>,

    /// Platform specific settings. Hints to OS for context creation, driver-specific
    /// settings etc.
    pub platform: Platform,

    pub hide_toolbar: bool,
    pub transparency: bool,
    pub blur: bool,
    pub hide_toolbar_buttons: bool,
    pub tab_identifier: Option<String>,
}

/// Icon image in three levels of detail.
pub struct Icon {
    pub inner: [u8; 410598],
}

impl Icon {
    pub fn logo() -> Icon {
        Icon {
            inner: *crate::resources::images::LOGO_ICON,
        }
    }
}
// Printing 64x64 array with a default formatter is not meaningfull,
// so debug will skip the data fields of an Icon
impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon").finish()
    }
}

// reasonable defaults for PC and mobiles are slightly different
impl Default for Conf {
    fn default() -> Conf {
        Conf {
            window_title: "".to_owned(),
            window_width: 800,
            window_height: 600,
            high_dpi: true,
            fullscreen: false,
            blur: false,
            transparency: false,
            hide_toolbar: false,
            sample_count: 1,
            window_resizable: true,
            icon: Some(Icon::logo()),
            platform: Default::default(),
            hide_toolbar_buttons: false,
            tab_identifier: None,
        }
    }
}
