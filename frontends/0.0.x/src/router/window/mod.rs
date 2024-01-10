#[cfg(target_os = "macos")]
use {
    cocoa::base::{id, NO, YES},
    objc::{msg_send, sel, sel_impl},
};

#[cfg(target_os = "macos")]
use raw_window_handle::HasRawWindowHandle;
#[cfg(target_os = "macos")]
use raw_window_handle::RawWindowHandle;
use rio_backend::config::window::{Decorations, WindowMode};
use rio_backend::config::Config;
use std::rc::Rc;
use winit::window::{CursorIcon, Fullscreen, Icon, ImePurpose, Window, WindowBuilder};

pub const LOGO_ICON: &[u8; 410598] = include_bytes!("./resources/images/rio-logo.ico");
// Terminal W/H contraints
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 150;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 300;

#[cfg(target_os = "macos")]
fn set_has_shadow(window: &Window, has_shadows: bool) {
    let raw_window = match window.raw_window_handle() {
        RawWindowHandle::AppKit(handle) => handle.ns_window as id,
        _ => return,
    };

    let value = if has_shadows { YES } else { NO };
    unsafe {
        let _: id = msg_send![raw_window, setHasShadow: value];
    }
}

#[cfg(all(
    any(feature = "wayland", feature = "x11"),
    not(any(target_os = "macos", windows))
))]
pub const APPLICATION_ID: &str = "rio";

pub fn create_window_builder(
    title: &str,
    config: &Rc<Config>,
    #[allow(unused)] tab_id: Option<String>,
) -> WindowBuilder {
    let image_icon = image::load_from_memory(LOGO_ICON).unwrap();
    let icon = Icon::from_rgba(
        image_icon.to_rgba8().into_raw(),
        image_icon.width(),
        image_icon.height(),
    )
    .unwrap();

    let mut window_builder = WindowBuilder::new()
        .with_title(title)
        .with_min_inner_size(winit::dpi::LogicalSize {
            width: DEFAULT_MINIMUM_WINDOW_WIDTH,
            height: DEFAULT_MINIMUM_WINDOW_HEIGHT,
        })
        .with_resizable(true)
        .with_decorations(true)
        .with_transparent(config.window.background_opacity < 1.)
        .with_blur(config.window.blur)
        .with_window_icon(Some(icon));

    match config.window.decorations {
        Decorations::Disabled => {
            window_builder = window_builder.with_decorations(false);
        }
        Decorations::Transparent => {
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::WindowBuilderExtMacOS;
                window_builder = window_builder.with_titlebar_transparent(true)
            }
        }
        Decorations::Buttonless => {
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::WindowBuilderExtMacOS;
                window_builder = window_builder.with_titlebar_buttons_hidden(true)
            }
        }
        _ => {}
    };

    #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
    {
        use winit::platform::x11::WindowBuilderExtX11;
        window_builder = window_builder.with_name(APPLICATION_ID, "");
    }

    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    {
        use winit::platform::wayland::WindowBuilderExtWayland;
        window_builder = window_builder.with_name(APPLICATION_ID, "");
    }

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowBuilderExtMacOS;
        // MacOS is always transparent
        window_builder = window_builder.with_transparent(true);

        if config.navigation.is_native() {
            if let Some(identifier) = tab_id {
                window_builder = window_builder.with_tabbing_identifier(&identifier);
            }
        } else {
            window_builder = window_builder
                .with_title_hidden(true)
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true);
        }
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
            window_builder = window_builder.with_inner_size(winit::dpi::LogicalSize {
                width: config.window.width,
                height: config.window.height,
            })
        }
    };

    window_builder
}

pub fn configure_window(winit_window: Window, config: &Rc<Config>) -> Window {
    let current_mouse_cursor = CursorIcon::Text;
    winit_window.set_cursor_icon(current_mouse_cursor);

    // https://docs.rs/winit/latest/winit;/window/enum.ImePurpose.html#variant.Terminal
    winit_window.set_ime_purpose(ImePurpose::Terminal);
    winit_window.set_ime_allowed(true);

    // TODO: Update ime position based on cursor
    // winit_window.set_ime_cursor_area(winit::dpi::PhysicalPosition::new(500.0, 500.0), winit::dpi::LogicalSize::new(400, 400));

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
        use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};

        match config.option_as_alt.to_lowercase().as_str() {
            "both" => winit_window.set_option_as_alt(OptionAsAlt::Both),
            "left" => winit_window.set_option_as_alt(OptionAsAlt::OnlyLeft),
            "right" => winit_window.set_option_as_alt(OptionAsAlt::OnlyRight),
            _ => {}
        }
    }

    let is_transparent = config.window.background_opacity < 1.;
    winit_window.set_transparent(is_transparent);
    #[cfg(target_os = "macos")]
    set_has_shadow(&winit_window, !is_transparent);

    winit_window.set_blur(config.window.blur);

    winit_window
}
