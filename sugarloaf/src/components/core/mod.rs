pub mod image;
// pub mod svg;
pub mod buffer;
pub mod shapes;

#[inline]
#[rustfmt::skip]
pub fn orthographic_projection(width: u32, height: u32) -> [f32; 16] {
    let h = height as f32;
    let w = width as f32;

    [
        2.0 / w, 0.0,      0.0, 0.0,
        0.0,     -2.0 / h, 0.0, 0.0,
        0.0,     0.0,      1.0, 0.0,
        -1.0,    1.0,      0.0, 1.0,
    ]
}
