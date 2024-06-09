extern crate criterion;
extern crate sugarloaf;

use crate::layout::SugarloafLayout;
use criterion::{criterion_group, criterion_main, Criterion};
// use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use sugarloaf::*;
use winit::dpi::LogicalSize;
use winit::event::Event;
use winit::event::WindowEvent;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::WindowAttributes;

fn bench_sugar_pile_with_screen(c: &mut Criterion) {
    const NUM: usize = 10_000;

    let mut event_loop = EventLoop::new().unwrap();
    let width = 400.0;
    let height = 400.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Bench")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 40.;
    let line_height = 2.0;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (10.0, 10.0, 0.0),
        scale_factor as f32,
        font_size,
        line_height,
    );

    let size = window.inner_size();
    let sugarloaf_window = SugarloafWindow {
        // handle: window.window_handle().unwrap().into(),
        // display: window.display_handle().unwrap().into(),
        handle: window.raw_window_handle(),
        display: window.raw_display_handle(),
        scale: scale_factor as f32,
        size: SugarloafWindowSize {
            width: size.width as f32,
            height: size.height as f32,
        },
    };

    let font_library = sugarloaf::font::FontLibrary::default();
    let mut sugarloaf = futures::executor::block_on(Sugarloaf::new(
        sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        &font_library,
        sugarloaf_layout,
    ))
    .expect("Sugarloaf instance should be created");

    #[allow(deprecated)]
    let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
        event_loop_window_target.set_control_flow(ControlFlow::Wait);

        match event {
            Event::Resumed => {
                sugarloaf.set_background_color(wgpu::Color::RED);
                window.request_redraw();
            }
            Event::WindowEvent { event, .. } => {
                if let WindowEvent::RedrawRequested { .. } = event {
                    c.bench_function("bench_sugar_pile_with_screen", |b| {
                        b.iter(|| {
                            sugarloaf.start_line();
                            for _i in 0..NUM {
                                sugarloaf.insert_on_current_line(&Sugar {
                                    content: 'a',
                                    foreground_color: [1.0, 1.0, 1.0, 1.0],
                                    background_color: Some([0.0, 1.0, 1.0, 1.0]),
                                    ..Sugar::default()
                                });
                            }

                            sugarloaf.finish_line();
                            sugarloaf.render();
                        })
                    });
                }
            }
            _ => {
                event_loop_window_target.exit();
            }
        }
    });
}

criterion_group!(benches, bench_sugar_pile_with_screen);
criterion_main!(benches);
