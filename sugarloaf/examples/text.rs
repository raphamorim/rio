extern crate tokio;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use sugarloaf::{
    layout::SugarloafLayout, ContentBuilder, FragmentStyle, FragmentStyleDecoration,
    Sugarloaf, SugarloafWindow, SugarloafWindowSize, UnderlineInfo, UnderlineShape,
};
use winit::event_loop::ControlFlow;
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::EventLoop,
    window::WindowAttributes,
};

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new().unwrap();
    let width = 400.0;
    let height = 300.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Text example")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 30.;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (10.0, 10.0, 0.0),
        scale_factor as f32,
        font_size,
        1.0,
    );

    let size = window.inner_size();
    let sugarloaf_window = SugarloafWindow {
        handle: window.window_handle().unwrap().into(),
        display: window.display_handle().unwrap().into(),
        scale: scale_factor as f32,
        size: SugarloafWindowSize {
            width: size.width as f32,
            height: size.height as f32,
        },
    };

    let mut sugarloaf = Sugarloaf::new(
        sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        &sugarloaf::font::FontLibrary::default(),
        sugarloaf_layout,
    )
    .await
    .expect("Sugarloaf instance should be created");

    #[allow(deprecated)]
    let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        match event {
            Event::Resumed => {
                sugarloaf.set_background_color(wgpu::Color::RED);
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => event_loop_window_target.exit(),
                WindowEvent::ScaleFactorChanged {
                    // mut inner_size_writer,
                    scale_factor,
                    ..
                } => {
                    let scale_factor_f32 = scale_factor as f32;
                    let new_inner_size = window.inner_size();
                    sugarloaf.rescale(scale_factor_f32);
                    sugarloaf.resize(new_inner_size.width, new_inner_size.height);
                    window.request_redraw();
                }
                winit::event::WindowEvent::Resized(new_size) => {
                    sugarloaf.resize(new_size.width, new_size.height);
                    window.request_redraw();
                }
                winit::event::WindowEvent::RedrawRequested { .. } => {
                    let mut content = ContentBuilder::default();
                    content.add_text(
                        "Sugarloaf",
                        FragmentStyle {
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    );
                    content.finish_line();
                    content.add_text(
                        "㏑¼",
                        FragmentStyle {
                            color: [0.0, 0.0, 0.0, 1.0],
                            background_color: Some([1.0, 1.0, 1.0, 1.0]),
                            width: 2.0,
                            ..FragmentStyle::default()
                        },
                    );
                    content.add_text(
                        "🥶",
                        FragmentStyle {
                            color: [1.0, 0.0, 1.0, 1.0],
                            background_color: Some([0.3, 0.5, 1.0, 1.0]),
                            width: 2.0,
                            ..FragmentStyle::default()
                        },
                    );
                    content.finish_line();
                    content.add_text(
                        "ok ",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    offset: -2.0,
                                    size: 1.0,
                                    is_doubled: false,
                                    shape: UnderlineShape::Regular,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    );
                    content.add_text(
                        "curly",
                        FragmentStyle {
                            decoration: Some(FragmentStyleDecoration::Underline(
                                UnderlineInfo {
                                    offset: -2.0,
                                    size: 1.0,
                                    is_doubled: false,
                                    shape: UnderlineShape::Curly,
                                },
                            )),
                            color: [1.0, 1.0, 1.0, 1.0],
                            background_color: Some([0.0, 0.0, 0.0, 1.0]),
                            ..FragmentStyle::default()
                        },
                    );
                    content.finish_line();
                    sugarloaf.set_content(content.build());

                    sugarloaf.render();
                }
                _ => (),
            },
            _ => {
                event_loop_window_target.set_control_flow(ControlFlow::Wait);
            }
        }
    });
}
