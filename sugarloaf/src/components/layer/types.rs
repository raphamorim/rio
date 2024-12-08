use crate::components::core::image;
// use crate::components::core::svg;
use crate::components::core::shapes::Rectangle;

#[derive(Debug, Clone)]
/// A raster image.
pub struct Raster {
    /// The handle of a raster image.
    pub handle: image::Handle,

    /// The bounds of the image.
    pub bounds: Rectangle,
}
