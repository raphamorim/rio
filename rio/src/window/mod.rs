pub mod input;

use crate::shared::{
    DEFAULT_MINIMUM_WINDOW_HEIGHT, DEFAULT_MINIMUM_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT,
    DEFAULT_WINDOW_WIDTH, LOGO_ICON,
};

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
