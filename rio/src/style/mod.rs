pub const LOGO_ICON: &[u8; 102762] = include_bytes!("./images/logo.ico");

pub const FONT_FIRA_MONO: &[u8; 170204] =
    include_bytes!("./fonts/Fira_Mono/FiraMono-Regular.ttf");

// Terminal W/H contraints
pub const DEFAULT_WINDOW_HEIGHT: i32 = 400;
pub const DEFAULT_WINDOW_WIDTH: i32 = 600;
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 300;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 400;

// #151515
pub const DEFAULT_COLOR_BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.021,
    g: 0.021,
    b: 0.021,
    a: 1.0,
};

// #d35100 (todo)
// pub const DEFAULT_COLOR_LINE: wgpu::Color = wgpu::Color {
//     r: 0.8274509804,
//     g: 0.3176470588,
//     b: 0.0,
//     a: 1.0,
// };

// #F8A145 (todo)
// #F07900 (todo)

pub fn create_window_builder(title: &str) -> winit::window::WindowBuilder {
    use winit::window::Icon;

    let image_icon = image::load_from_memory(LOGO_ICON).unwrap();
    let icon = Icon::from_rgba(
        image_icon.to_rgba8().into_raw(),
        image_icon.width(),
        image_icon.height(),
    )
    .unwrap();

    let mut window_builder = winit::window::WindowBuilder::new()
        .with_title(title)
        .with_inner_size(winit::dpi::LogicalSize {
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
        })
        .with_min_inner_size(winit::dpi::LogicalSize {
            width: DEFAULT_MINIMUM_WINDOW_WIDTH,
            height: DEFAULT_MINIMUM_WINDOW_HEIGHT,
        })
        .with_resizable(true)
        .with_decorations(true)
        .with_window_icon(Some(icon));

    {
        use winit::platform::macos::WindowBuilderExtMacOS;

        window_builder = window_builder
            .with_title_hidden(true)
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true);
    }

    window_builder
}
