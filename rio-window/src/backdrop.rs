/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Source for obtaining a backdrop texture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackdropSource {
    None,
    Os,
    Video,
    Scene3D,
}

/// Provides a backdrop texture for refraction or other effects.
pub trait BackdropProvider {
    /// Begin a frame and return a texture view for the specified rectangle.
    fn begin_frame(&mut self, rect: PhysicalRect) -> Option<wgpu::TextureView>;
}
