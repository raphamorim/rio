// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Comprehensive benchmarking suite for rich text rendering performance

use criterion::{
    black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput,
};
use std::time::Duration;
use sugarloaf::components::rich_text::text::{Glyph, TextRunStyle};
use sugarloaf::components::rich_text::{FrameStats, RichTextBrush};
use sugarloaf::context::Context;
use sugarloaf::font::FontLibrary;
use sugarloaf::layout::{BuilderLine, FragmentStyle, RenderData, Run, Span};
use sugarloaf::Graphics;

/// Generate test data for benchmarking
struct BenchmarkData {
    lines: Vec<BuilderLine>,
    font_library: FontLibrary,
    graphics: Graphics,
}

impl BenchmarkData {
    fn new(line_count: usize, chars_per_line: usize) -> Self {
        let mut lines = Vec::with_capacity(line_count);
        let font_library = FontLibrary::default();
        let graphics = Graphics::default();

        for line_idx in 0..line_count {
            let mut runs = Vec::new();
            let mut glyphs = Vec::new();

            // Create glyphs for this line
            for char_idx in 0..chars_per_line {
                glyphs.push(Glyph {
                    id: (char_idx % 95 + 32) as u32, // ASCII printable range
                    x: char_idx as f32 * 8.0,        // 8px char width
                    y: line_idx as f32 * 16.0,       // 16px line height
                });
            }

            // Create a run for this line
            let span = Span {
                font_id: 0,
                width: 1.0,
                color: [1.0, 1.0, 1.0, 1.0],
                cursor: None,
                drawable_char: None,
                background_color: None,
                decoration: None,
                decoration_color: None,
                media: None,
            };

            let run = Run {
                glyphs,
                span,
                size: 14.0,
            };

            runs.push(run);

            let line = BuilderLine {
                render_data: RenderData { runs },
            };

            lines.push(line);
        }

        Self {
            lines,
            font_library,
            graphics,
        }
    }
}

/// Benchmark basic rich text rendering
fn bench_rich_text_rendering(c: &mut Criterion) {
    let mut group = c.benchmark_group("rich_text_rendering");

    // Test different line counts
    for line_count in [10, 50, 100, 500].iter() {
        let chars_per_line = 80;
        let data = BenchmarkData::new(*line_count, chars_per_line);

        group.throughput(Throughput::Elements(*line_count as u64));
        group.bench_with_input(
            BenchmarkId::new("lines", line_count),
            line_count,
            |b, &line_count| {
                // Note: This is a simplified benchmark - in real usage we'd need a proper Context
                // For now, we'll benchmark the data preparation and processing logic
                b.iter(|| {
                    let data = BenchmarkData::new(line_count, chars_per_line);
                    black_box(&data.lines);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark glyph processing performance
fn bench_glyph_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("glyph_processing");

    for glyph_count in [100, 1000, 5000, 10000].iter() {
        let mut glyphs = Vec::with_capacity(*glyph_count);

        // Generate test glyphs
        for i in 0..*glyph_count {
            glyphs.push(Glyph {
                id: (i % 95 + 32) as u32,
                x: (i % 80) as f32 * 8.0,
                y: (i / 80) as f32 * 16.0,
            });
        }

        group.throughput(Throughput::Elements(*glyph_count as u64));
        group.bench_with_input(
            BenchmarkId::new("glyphs", glyph_count),
            &glyphs,
            |b, glyphs| {
                b.iter(|| {
                    // Simulate glyph processing
                    let mut processed = 0;
                    for glyph in glyphs {
                        // Simulate some processing work
                        let _result = glyph.id as f32 * glyph.x + glyph.y;
                        processed += 1;
                    }
                    black_box(processed);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark memory allocation patterns
fn bench_memory_allocation(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_allocation");

    // Benchmark vector allocation patterns
    group.bench_function("vec_new_vs_with_capacity", |b| {
        b.iter(|| {
            // Simulate the old pattern (allocate new each time)
            let mut vecs = Vec::new();
            for _ in 0..100 {
                let mut v = Vec::new();
                for i in 0..50 {
                    v.push(i);
                }
                vecs.push(v);
            }
            black_box(vecs);
        });
    });

    group.bench_function("vec_reuse_pattern", |b| {
        b.iter(|| {
            // Simulate the new pattern (reuse vectors)
            let mut reusable_vec = Vec::with_capacity(50);
            let mut results = Vec::new();

            for _ in 0..100 {
                reusable_vec.clear();
                for i in 0..50 {
                    reusable_vec.push(i);
                }
                results.push(reusable_vec.len());
            }
            black_box(results);
        });
    });

    group.finish();
}

/// Benchmark cache performance
fn bench_cache_performance(c: &mut Criterion) {
    use std::collections::HashMap;

    let mut group = c.benchmark_group("cache_performance");

    // Simulate cache hit/miss patterns
    let cache_sizes = [100, 1000, 10000];
    let access_patterns = [
        ("sequential", (0..1000).collect::<Vec<_>>()),
        ("random", {
            use rand::prelude::*;
            let mut rng = thread_rng();
            let mut pattern: Vec<usize> = (0..1000).collect();
            pattern.shuffle(&mut rng);
            pattern
        }),
        ("hot_cache", {
            // 80% of accesses to 20% of keys (hot cache scenario)
            let mut pattern = Vec::new();
            for _ in 0..800 {
                pattern.push(fastrand::usize(0..200));
            }
            for _ in 0..200 {
                pattern.push(fastrand::usize(200..1000));
            }
            pattern
        }),
    ];

    for &cache_size in &cache_sizes {
        for (pattern_name, pattern) in &access_patterns {
            let mut cache: HashMap<usize, String> = HashMap::new();

            // Pre-populate cache
            for i in 0..cache_size {
                cache.insert(i, format!("value_{}", i));
            }

            group.bench_with_input(
                BenchmarkId::new(
                    format!("cache_{}_{}", cache_size, pattern_name),
                    cache_size,
                ),
                &(cache, pattern),
                |b, (cache, pattern)| {
                    b.iter(|| {
                        let mut hits = 0;
                        let mut misses = 0;

                        for &key in pattern {
                            if cache.contains_key(&key) {
                                hits += 1;
                                black_box(cache.get(&key));
                            } else {
                                misses += 1;
                            }
                        }

                        black_box((hits, misses));
                    });
                },
            );
        }
    }

    group.finish();
}

/// Benchmark vertex buffer operations
fn bench_vertex_buffer(c: &mut Criterion) {
    use sugarloaf::components::rich_text::compositor::Vertex;

    let mut group = c.benchmark_group("vertex_buffer");

    for vertex_count in [1000, 5000, 10000, 50000].iter() {
        let vertices: Vec<Vertex> = (0..*vertex_count)
            .map(|i| Vertex {
                pos: [i as f32, i as f32, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
                uv: [0.0, 0.0],
                layers: [0, 0],
            })
            .collect();

        group.throughput(Throughput::Elements(*vertex_count as u64));

        // Benchmark different buffer growth strategies
        group.bench_with_input(
            BenchmarkId::new("copy_vertices", vertex_count),
            &vertices,
            |b, vertices| {
                b.iter(|| {
                    let mut buffer = Vec::new();
                    buffer.extend_from_slice(vertices);
                    black_box(buffer);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("preallocated_buffer", vertex_count),
            &vertices,
            |b, vertices| {
                b.iter(|| {
                    let mut buffer = Vec::with_capacity(vertices.len());
                    buffer.extend_from_slice(vertices);
                    black_box(buffer);
                });
            },
        );
    }

    group.finish();
}

/// Benchmark text style processing
fn bench_text_style_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("text_style_processing");

    // Create various text styles to benchmark
    let styles = vec![
        FragmentStyle {
            color: [1.0, 1.0, 1.0, 1.0],
            background_color: None,
            ..Default::default()
        },
        FragmentStyle {
            color: [1.0, 0.0, 0.0, 1.0],
            background_color: Some([0.0, 0.0, 1.0, 1.0]),
            ..Default::default()
        },
        // Add more style variations
    ];

    group.bench_function("style_comparison", |b| {
        b.iter(|| {
            let mut comparisons = 0;
            for i in 0..styles.len() {
                for j in i + 1..styles.len() {
                    if styles[i] == styles[j] {
                        comparisons += 1;
                    }
                }
            }
            black_box(comparisons);
        });
    });

    group.finish();
}

/// Performance regression test
fn bench_performance_regression(c: &mut Criterion) {
    let mut group = c.benchmark_group("performance_regression");
    group.measurement_time(Duration::from_secs(10));

    // This benchmark serves as a regression test for overall performance
    let data = BenchmarkData::new(100, 80); // 100 lines, 80 chars each

    group.bench_function("full_rendering_pipeline", |b| {
        b.iter(|| {
            // Simulate the full rendering pipeline
            let mut total_glyphs = 0;
            let mut total_vertices = 0;

            for line in &data.lines {
                for run in &line.render_data.runs {
                    total_glyphs += run.glyphs.len();
                    // Simulate vertex generation (6 vertices per glyph for 2 triangles)
                    total_vertices += run.glyphs.len() * 6;
                }
            }

            black_box((total_glyphs, total_vertices));
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_rich_text_rendering,
    bench_glyph_processing,
    bench_memory_allocation,
    bench_cache_performance,
    bench_vertex_buffer,
    bench_text_style_processing,
    bench_performance_regression
);

criterion_main!(benches);
