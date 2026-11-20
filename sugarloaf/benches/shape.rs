// CoreText shaping and rasterization per call, the cost of a run cache
// miss in the grid renderer. Every miss shapes one run and rasterizes
// its new glyphs, so these are the numbers behind a frame of unique
// rows. macOS only; the bench is a no-op elsewhere.

use criterion::{criterion_group, criterion_main, Criterion};

#[cfg(target_os = "macos")]
fn bench(c: &mut Criterion) {
    use sugarloaf::font::constants::FONT_CASCADIA_CODE_NF;
    use sugarloaf::font::macos::{
        font_metrics, rasterize_glyph, shape_text, shape_text_utf16, FontHandle,
    };

    let handle =
        FontHandle::from_static_bytes(FONT_CASCADIA_CODE_NF).expect("bundled font");
    let size = 28.0f32;
    let short: Vec<u16> = "ls -la".encode_utf16().collect();
    let long: Vec<u16> =
        "the quick brown fox jumps over the lazy dog 0123456789 lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor"
            .encode_utf16()
            .collect();
    let glyph_a =
        shape_text_utf16(&handle, &"a".encode_utf16().collect::<Vec<_>>(), size)[0].id;

    let mut group = c.benchmark_group("shape");
    group.bench_function("utf16 run of 6", |b| {
        b.iter(|| shape_text_utf16(&handle, &short, size))
    });
    group.bench_function("utf16 run of 130", |b| {
        b.iter(|| shape_text_utf16(&handle, &long, size))
    });
    group.bench_function("utf8 label of 9", |b| {
        b.iter(|| shape_text(&handle, "Tab title", size))
    });
    group.bench_function("rasterize a", |b| {
        b.iter(|| rasterize_glyph(&handle, glyph_a, size, false, false, false))
    });
    group.bench_function("rasterize a, synthetic italic", |b| {
        b.iter(|| rasterize_glyph(&handle, glyph_a, size, false, true, false))
    });
    group.bench_function("font metrics", |b| b.iter(|| font_metrics(&handle, size)));
    group.finish();
}

#[cfg(not(target_os = "macos"))]
fn bench(_c: &mut Criterion) {}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30);
    targets = bench
}
criterion_main!(benches);
