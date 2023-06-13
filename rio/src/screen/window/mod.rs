pub const LOGO_ICON: &[u8; 119202] = include_bytes!("./resources/images/logo-macos.ico");
// Terminal W/H contraints
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 150;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 300;

pub const DEFAULT_HEIGHT: i32 = 400;
pub const DEFAULT_WIDTH: i32 = 600;

pub fn create_window_builder(title: &str) -> winit::window::WindowBuilder {
    use winit::window::Icon;

    let image_icon = image::load_from_memory(LOGO_ICON).unwrap();
    let icon = Icon::from_rgba(
        image_icon.to_rgba8().into_raw(),
        image_icon.width(),
        image_icon.height(),
    )
    .unwrap();

    #[allow(unused_mut)]
    let mut window_builder = winit::window::WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize {
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
        })
        .with_min_inner_size(winit::dpi::LogicalSize {
            width: DEFAULT_MINIMUM_WINDOW_WIDTH,
            height: DEFAULT_MINIMUM_WINDOW_HEIGHT,
        })
        .with_resizable(true)
        .with_decorations(true)
        .with_window_icon(Some(icon));

    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        // use winit::platform::unix::WindowBuilderExtUnix;
        // window_builder = window_builder.with_name(title);
    }

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowBuilderExtMacOS;
        window_builder = window_builder
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true);
    }

    window_builder
}
