use crate::native::macos::MacosDisplay;
use raw_window_handle::HasRawDisplayHandle;
use raw_window_handle::HasRawWindowHandle;
use sugarloaf::layout::SugarloafLayout;
use sugarloaf::Sugarloaf;
use sugarloaf::SugarloafWindow;
use sugarloaf::SugarloafWindowSize;

#[inline]
pub fn create_sugarloaf_instance(
    display: MacosDisplay,
    width: f32,
    height: f32,
    scale_factor: f32,
) -> Sugarloaf {
    let font_size = 18.;
    let line_height = 1.0;
    let sugarloaf_layout = SugarloafLayout::new(
        width,
        height,
        (10.0, 10.0, 0.0),
        scale_factor,
        font_size,
        line_height,
        (2, 1),
    );

    let raw_window_handle = display.raw_window_handle();
    let raw_display_handle = display.raw_display_handle();
    let sugarloaf_window = SugarloafWindow {
        handle: raw_window_handle,
        display: raw_display_handle,
        scale: scale_factor,
        size: SugarloafWindowSize {
            width: width as u32,
            height: height as u32,
        },
    };

    let mut sugarloaf = futures::executor::block_on(Sugarloaf::new(
        &sugarloaf_window,
        wgpu::PowerPreference::HighPerformance,
        sugarloaf::font::fonts::SugarloafFonts::default(),
        sugarloaf_layout,
        None,
    ))
    .expect("Sugarloaf instance should be created");

    sugarloaf.set_background_color(wgpu::Color::RED);
    sugarloaf.calculate_bounds();

    sugarloaf
}
