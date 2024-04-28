extern crate criterion;
extern crate sugarloaf;

use crate::layout::SugarloafLayout;
use criterion::{criterion_group, criterion_main, Criterion};
use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};
use sugarloaf::core::Sugar;
use sugarloaf::*;
use winit::dpi::LogicalSize;
use winit::event_loop::EventLoop;
use winit::window::WindowAttributes;

fn bench_sugar_pile(c: &mut Criterion) {
    const NUM: usize = 100_000;

    let event_loop = EventLoop::new().unwrap();
    let width = 1200.0;
    let height = 800.0;

    let window_attribute = WindowAttributes::default()
        .with_title("Bench")
        .with_inner_size(LogicalSize::new(width, height))
        .with_resizable(true);
    #[allow(deprecated)]
    let window = event_loop.create_window(window_attribute).unwrap();

    let scale_factor = window.scale_factor();
    let font_size = 60.;
    let line_height = 1.0;

    let size = window.inner_size();
    let sugarloaf_window = SugarloafWindow {
        handle: window.raw_window_handle(),
        display: window.raw_display_handle(),
        scale: scale_factor as f32,
        size: SugarloafWindowSize {
            width: size.width,
            height: size.height,
        },
    };

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
        &sugarloaf_window,
        sugarloaf::SugarloafRenderer::default(),
        sugarloaf::font::fonts::SugarloafFonts::default(),
        sugarloaf_layout,
        None,
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
