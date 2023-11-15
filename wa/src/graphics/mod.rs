// use std::error::Error;
// use sugarloaf::SugarloafWindow;
// use sugarloaf::SugarloafWindowSize;
// use sugarloaf::layout::SugarloafLayout;
// use sugarloaf::Sugarloaf;

// #[inline]
// pub fn create_sugarloaf_instance(
//     window_handle: raw_window_handle::handle, 
//     window_display: raw_window_handle::display,
//     width: f32,
//     height: f32,
//     scale_factor: f32
// ) -> Result<Sugarloaf, Box<dyn Error>> {
//     let sugarloaf_layout = SugarloafLayout::new(
//         width,
//         height,
//         (10.0, 10.0, 0.0),
//         scale_factor,
//         font_size,
//         line_height,
//         (2, 1),
//     );

//     let size = window.inner_size();
//     let sugarloaf_window = SugarloafWindow {
//         handle: window_handle,
//         display: window_display,
//         scale: scale_factor as f32,
//         size: SugarloafWindowSize {
//             width: width,
//             height: height,
//         },
//     };

//     let mut sugarloaf = Sugarloaf::new(
//         &sugarloaf_window,
//         wgpu::PowerPreference::HighPerformance,
//         sugarloaf::font::fonts::SugarloafFonts::default(),
//         // "Fira Code".to_string(),
//         // "Monaco".to_string(),
//         // "Space Mono".to_string(),
//         // "Menlo".to_string(),
//         sugarloaf_layout,
//         None,
//     )
//     .await
//     .expect("Sugarloaf instance should be created");   
// }