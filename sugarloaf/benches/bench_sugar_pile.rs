extern crate criterion;
extern crate sugarloaf;

use crate::layout::SugarloafLayout;
use criterion::{criterion_group, criterion_main, Criterion};
use sugarloaf::core::Sugar;
use sugarloaf::*;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;

fn bench_sugar_pile(c: &mut Criterion) {
    const NUM: usize = 100_000;

    let event_loop = EventLoop::new();
    let width = 1200.0;
    let height = 800.0;

    let window = WindowBuilder::new()
        .with_title("Bench")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 60.;
    let line_height = 1.0;

    let sugarloaf_layout = SugarloafLayout::new(
        width as f32,
        height as f32,
        (0.0, 0.0, 0.0),
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

    sugarloaf.set_background_color(wgpu::Color::RED);
    sugarloaf.calculate_bounds();

    c.bench_function("bench_sugar_pile", |b| {
        b.iter(|| {
            let mut pile = vec![];
            let mut pile2 = vec![];
            let mut pile3 = vec![];
            for _i in 0..NUM {
                pile.push(Sugar {
                    content: ' ',
                    foreground_color: [0.0, 0.0, 0.0, 1.0],
                    background_color: [0.0, 1.0, 1.0, 1.0],
                    style: None,
                    decoration: None,
                });

                pile2.push(Sugar {
                    content: '«',
                    foreground_color: [0.0, 0.0, 0.0, 1.0],
                    background_color: [0.0, 1.0, 1.0, 1.0],
                    style: None,
                    decoration: None,
                });

                pile3.push(Sugar {
                    content: '≥',
                    foreground_color: [0.0, 0.0, 0.0, 1.0],
                    background_color: [0.0, 1.0, 1.0, 1.0],
                    style: None,
                    decoration: None,
                });
            }

            sugarloaf.stack(pile);
            sugarloaf.stack(pile2);
            sugarloaf.stack(pile3);

            sugarloaf.render();
        })
    });
}

criterion_group!(benches, bench_sugar_pile);
criterion_main!(benches);
