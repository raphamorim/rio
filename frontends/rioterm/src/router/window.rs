use rio_backend::config::window::{Decorations, WindowMode};
use rio_backend::config::Config;
use rio_window::window::{
    CursorIcon, Fullscreen, Icon, ImePurpose, Window, WindowAttributes,
};

pub const LOGO_ICON: &[u8; 410598] = include_bytes!("./resources/images/rio-logo.ico");
// Terminal W/H constraints
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 200;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 300;

#[cfg(all(
    any(feature = "wayland", feature = "x11"),
    not(any(target_os = "macos", windows))
))]
pub const APPLICATION_ID: &str = "Rio";

pub fn create_window_builder(
    title: &str,
    config: &Config,
    #[allow(unused_variables)] tab_id: Option<&str>,
) -> WindowAttributes {
    let image_icon = image_rs::load_from_memory(LOGO_ICON).unwrap();
    let icon = Icon::from_rgba(
        image_icon.to_rgba8().into_raw(),
        image_icon.width(),
        image_icon.height(),
    )
    .unwrap();

    let mut window_builder = WindowAttributes::default()
        .with_title(title)
        .with_min_inner_size(rio_window::dpi::LogicalSize {
            width: DEFAULT_MINIMUM_WINDOW_WIDTH,
            height: DEFAULT_MINIMUM_WINDOW_HEIGHT,
        })
        .with_resizable(true)
        .with_decorations(true)
        .with_transparent(config.window.opacity < 1.)
        .with_blur(config.window.blur)
        .with_window_icon(Some(icon));

    match config.window.decorations {
        Decorations::Disabled => {
            window_builder = window_builder.with_decorations(false);
        }
        Decorations::Transparent => {
            #[cfg(target_os = "macos")]
            {
                use rio_window::platform::macos::WindowAttributesExtMacOS;
                window_builder = window_builder.with_titlebar_transparent(true)
            }
        }
        Decorations::Buttonless => {
            #[cfg(target_os = "macos")]
            {
                use rio_window::platform::macos::WindowAttributesExtMacOS;
                window_builder = window_builder.with_titlebar_buttons_hidden(true)
            }
        }
        _ => {}
    };

    #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
    {
        use rio_window::platform::x11::WindowAttributesExtX11;
        window_builder =
            window_builder.with_name(APPLICATION_ID.to_lowercase(), APPLICATION_ID);
    }

    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    {
        use rio_window::platform::wayland::WindowAttributesExtWayland;
        window_builder =
            window_builder.with_name(APPLICATION_ID.to_lowercase(), APPLICATION_ID);
    }

    #[cfg(target_os = "windows")]
    {
        use rio_window::platform::windows::WindowAttributesExtWindows;
        if let Some(use_undecorated_shadow) = config.window.windows_use_undecorated_shadow
        {
            window_builder =
                window_builder.with_undecorated_shadow(use_undecorated_shadow);
        }

        if let Some(use_no_redirection_bitmap) =
            config.window.windows_use_no_redirection_bitmap
        {
            // This sets WS_EX_NOREDIRECTIONBITMAP.
            window_builder =
                window_builder.with_no_redirection_bitmap(use_no_redirection_bitmap);
        }
    }

    #[cfg(target_os = "macos")]
    {
        use rio_window::platform::macos::WindowAttributesExtMacOS;
        // MacOS is always transparent
        window_builder = window_builder.with_transparent(true);

        // Configure colorspace
        window_builder = window_builder
            .with_colorspace(config.window.colorspace.to_rio_window_colorspace());

        if config.navigation.is_native() {
            if let Some(identifier) = tab_id {
                window_builder = window_builder
                    .with_tabbing_identifier(identifier)
                    .with_unified_titlebar(config.window.macos_use_unified_titlebar);
            }
        } else {
            window_builder = window_builder
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use rio_window::platform::windows::WindowAttributesExtWindows;
        // On windows cloak (hide) the window initially, we later reveal it after the first draw.
        // This is a workaround to hide the "white flash" that occurs during application startup.
        window_builder = window_builder.with_cloaked(true);
    }

    match config.window.mode {
        WindowMode::Fullscreen => {
            window_builder =
                window_builder.with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        WindowMode::Maximized => {
            window_builder = window_builder.with_maximized(true);
        }
        _ => {
            window_builder =
                window_builder.with_inner_size(rio_window::dpi::LogicalSize {
                    width: config.window.width,
                    height: config.window.height,
                })
        }
    };

    window_builder
}

pub fn configure_window(winit_window: &Window, config: &Config) {
    let current_mouse_cursor = CursorIcon::Text;
    winit_window.set_cursor(current_mouse_cursor);

    // https://docs.rs/winit/latest/winit;/window/enum.ImePurpose.html#variant.Terminal
    winit_window.set_ime_purpose(ImePurpose::Terminal);
    winit_window.set_ime_allowed(true);

    // TODO: Update ime position based on cursor
    // winit_window.set_ime_cursor_area(rio_window::dpi::PhysicalPosition::new(500.0, 500.0), rio_window::dpi::LogicalSize::new(400, 400));

    // This will ignore diacritical marks and accent characters from
    // being processed as received characters. Instead, the input
    // device's raw character will be placed in event queues with the
    // Alt modifier set.
    #[cfg(target_os = "macos")]
    {
        // OnlyLeft - The left `Option` key is treated as `Alt`.
        // OnlyRight - The right `Option` key is treated as `Alt`.
        // Both - Both `Option` keys are treated as `Alt`.
        // None - No special handling is applied for `Option` key.
        use rio_window::platform::macos::{OptionAsAlt, WindowExtMacOS};

        match config.option_as_alt.to_lowercase().as_str() {
            "both" => winit_window.set_option_as_alt(OptionAsAlt::Both),
            "left" => winit_window.set_option_as_alt(OptionAsAlt::OnlyLeft),
            "right" => winit_window.set_option_as_alt(OptionAsAlt::OnlyRight),
            _ => {}
        }
    }

    let is_transparent = config.window.opacity < 1.;
    winit_window.set_transparent(is_transparent);

    #[cfg(target_os = "macos")]
    {
        use rio_window::platform::macos::WindowExtMacOS;
        let bg_color = config.colors.background.1;
        winit_window.set_background_color(
            bg_color.r,
            bg_color.g,
            bg_color.b,
            config.window.opacity as f64,
        );

        if !config.window.macos_use_shadow {
            winit_window.set_has_shadow(false);
        }
    }

    #[cfg(target_os = "windows")]
    {
        use rio_backend::config::window::WindowsCornerPreference;
        use rio_window::platform::windows::WindowExtWindows;

        if let Some(with_corner_preference) = &config.window.windows_corner_preference {
            let preference = match with_corner_preference {
                WindowsCornerPreference::Default => {
                    rio_window::platform::windows::CornerPreference::Default
                }
                WindowsCornerPreference::DoNotRound => {
                    rio_window::platform::windows::CornerPreference::DoNotRound
                }
                WindowsCornerPreference::Round => {
                    rio_window::platform::windows::CornerPreference::Round
                }
                WindowsCornerPreference::RoundSmall => {
                    rio_window::platform::windows::CornerPreference::RoundSmall
                }
            };

            winit_window.set_corner_preference(preference);
        }
    }
    if let Some(title) = &config.title.placeholder {
        winit_window.set_title(title);
    }

    winit_window.set_blur(config.window.blur);
}
