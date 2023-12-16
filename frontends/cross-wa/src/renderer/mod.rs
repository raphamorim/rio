use crate::constants;
use sugarloaf::layout::SugarloafLayout;
use sugarloaf::Sugarloaf;
use sugarloaf::SugarloafWindow;
use sugarloaf::SugarloafWindowSize;

#[inline]
pub fn create_sugarloaf_instance(
    handle: raw_window_handle::RawWindowHandle,
    display: raw_window_handle::RawDisplayHandle,
    width: f32,
    height: f32,
    scale_factor: f32,
    font_size: f32,
    background_color: wgpu::Color,
    padding_x: f32,
    padding_y: (f32, f32),
) -> Sugarloaf {
    let line_height = 1.0;
    let min_columns = 2;
    let min_rows = 2;

    let sugarloaf_layout = SugarloafLayout::new(
        width,
        height,
        (padding_x, padding_y.0, padding_y.1),
        scale_factor,
        font_size,
        line_height,
        (min_columns, min_rows),
    );

    let sugarloaf_window = SugarloafWindow {
        handle,
        display,
        scale: scale_factor,
        size: SugarloafWindowSize {
            width: width as u32,
            height: height as u32,
        },
    };

    let mut sugarloaf = futures::executor::block_on(Sugarloaf::new(
        &sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        sugarloaf::font::fonts::SugarloafFonts::default(),
        sugarloaf_layout,
        None,
    ))
    .expect("Sugarloaf instance should be created");

    sugarloaf.set_background_color(background_color);
    // TODO: Bug sugarloaf is not starting with right width/height
    // s.resize(width, height);
    sugarloaf.resize(width as u32, height as u32);
    sugarloaf.calculate_bounds();
    sugarloaf.render();

    sugarloaf
}

#[inline]
pub fn padding_top_from_config(config: &rio_backend::config::Config) -> f32 {
    #[cfg(not(target_os = "macos"))]
    {
        if config.navigation.is_placed_on_top() {
            return constants::PADDING_Y_WITH_TAB_ON_TOP;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if config.navigation.is_native() {
            return 0.0;
        }
    }

    constants::PADDING_Y
}

#[inline]
pub fn padding_bottom_from_config(config: &rio_backend::config::Config) -> f32 {
    if config.navigation.is_placed_on_bottom() {
        config.fonts.size
    } else {
        0.0
    }
}
