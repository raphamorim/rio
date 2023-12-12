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

    sugarloaf.set_background_color(wgpu::Color::RED);
    sugarloaf.calculate_bounds();

    sugarloaf
}
