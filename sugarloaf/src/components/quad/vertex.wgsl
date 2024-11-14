// This code was originally retired from iced-rs, which is licensed
// under MIT license https://github.com/iced-rs/iced/blob/master/LICENSE
// The code has suffered changes to fit on Sugarloaf architecture.

// Compute the normalized quad coordinates based on the vertex index.
fn vertex_position(vertex_index: u32) -> vec2<f32> {
    // #: 0 1 2 3 4 5
    // x: 1 1 0 0 0 1
    // y: 1 0 0 0 1 1
    return vec2<f32>((vec2(1u, 2u) + vertex_index) % vec2(6u) < vec2(3u));
}