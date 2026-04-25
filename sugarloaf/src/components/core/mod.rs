pub mod image;
pub mod uniforms;
// pub mod svg;
// `buffer` is a thin wrapper over `wgpu::Buffer` — gated together
// with the rest of the wgpu code.
#[cfg(feature = "wgpu")]
pub mod buffer;
pub mod shapes;

#[inline]
pub fn orthographic_projection(width: f32, height: f32) -> [f32; 16] {
    [
        2.0 / width,
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / height,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
        0.0,
        -1.0,
        1.0,
        0.0,
        1.0,
    ]
}
