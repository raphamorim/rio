use crate::components::core::image;
// use crate::components::core::svg;
use crate::components::core::shapes::Rectangle;

/// A raster or vector image.
#[derive(Debug, Clone)]
pub enum Image {
    /// A raster image.
    Raster {
        /// The handle of a raster image.
        handle: image::Handle,

        /// The bounds of the image.
        bounds: Rectangle,
    },
}
    // A vector image.
    // Vector {
    //     /// The handle of a vector image.
    //     handle: svg::Handle,

    //     /// The [`Color`] filter
    //     color: Option<wgpu::Color>,

    //     /// The bounds of the image.
    //     bounds: Rectangle,
    // },
// }