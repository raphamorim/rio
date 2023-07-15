use config::Config;
use std::rc::Rc;
use winit::window::{CursorIcon, Icon, ImePurpose, Window, WindowBuilder};

pub const LOGO_ICON: &[u8; 119202] =
    include_bytes!("./resources/images/embedded-logo.ico");
// Terminal W/H contraints
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 150;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 300;

pub fn create_window_builder(title: &str, config: &Rc<Config>) -> WindowBuilder {
    let image_icon = image::load_from_memory(LOGO_ICON).unwrap();
    let icon = Icon::from_rgba(
        image_icon.to_rgba8().into_raw(),
        image_icon.width(),
        image_icon.height(),
    )
    .unwrap();

    #[allow(unused_mut)]
    let mut window_builder = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize {
            width: config.window_width,
            height: config.window_height,
        })
        .with_min_inner_size(winit::dpi::LogicalSize {
            width: DEFAULT_MINIMUM_WINDOW_WIDTH,
            height: DEFAULT_MINIMUM_WINDOW_HEIGHT,
        })
        .with_resizable(true)
        .with_decorations(true)
        .with_window_icon(Some(icon));

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowBuilderExtMacOS;
        window_builder = window_builder
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_transparent(true)
            .with_fullsize_content_view(true);
    }

    window_builder
}

pub fn configure_window(winit_window: Window, config: &Rc<Config>) -> Window {
    let current_mouse_cursor = CursorIcon::Text;
    winit_window.set_cursor_icon(current_mouse_cursor);

    // https://docs.rs/winit/latest/winit;/window/enum.ImePurpose.html#variant.Terminal
    winit_window.set_ime_purpose(ImePurpose::Terminal);
    winit_window.set_ime_allowed(true);

    winit_window.set_transparent(config.window_opacity < 1.);

    // TODO: Update ime position based on cursor
    // winit_window.set_ime_position(winit::dpi::PhysicalPosition::new(500.0, 500.0));

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

    winit_window
}
