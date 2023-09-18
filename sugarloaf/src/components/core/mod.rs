pub mod image;
// pub mod svg;
pub mod buffer;
pub mod shapes;

#[inline]
pub fn orthographic_projection(width: u32, height: u32) -> [f32; 16] {
    [
        2.0 / width as f32,
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / height as f32,
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
