use winit::platform::run_return::EventLoopExtRunReturn;
extern crate criterion;
extern crate sugarloaf;

use crate::layout::SugarloafLayout;
use criterion::{criterion_group, criterion_main, Criterion};
use sugarloaf::core::Sugar;
use sugarloaf::*;
use winit::dpi::LogicalSize;
use winit::event::Event;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

fn bench_sugar_pile_with_screen(c: &mut Criterion) {
    const NUM: usize = 10_000;

    let mut event_loop = EventLoop::new();
    let width = 400.0;
    let height = 400.0;

    let window = WindowBuilder::new()
        .with_title("Bench")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(false)
        .build(&event_loop)
        .unwrap();

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
        (2, 1),
    );

    let mut sugarloaf = futures::executor::block_on(Sugarloaf::new(
        &window,
        wgpu::PowerPreference::LowPower,
        sugarloaf::font::constants::DEFAULT_FONT_NAME.to_string(),
        sugarloaf_layout,
    ))
    .expect("Sugarloaf instance should be created");

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                sugarloaf.set_background_color(wgpu::Color::RED);
                sugarloaf.calculate_bounds();
                window.request_redraw();
            }
            Event::RedrawRequested { .. } => {
                c.bench_function("bench_sugar_pile_with_screen", |b| {
                    b.iter(|| {
                        let mut pile = vec![];
                        for _i in 0..NUM {
                            pile.push(Sugar {
                                content: 'a',
                                foreground_color: [1.0, 1.0, 1.0, 1.0],
                                background_color: [0.0, 1.0, 1.0, 1.0],
                                style: None,
                                decoration: None,
                            });
                        }

                        sugarloaf.stack(pile);
                        sugarloaf.render();
                    })
                });
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Exit;
            }
        }
    });
}

criterion_group!(benches, bench_sugar_pile_with_screen);
criterion_main!(benches);
