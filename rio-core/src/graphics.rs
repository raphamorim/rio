//! Renderer-agnostic graphics payloads — relocated from
//! `sugarloaf::graphics`.
//!
//! These are the value types the image protocols (sixel, iTerm2, kitty)
//! produce: a decoded pixel blob plus its identity and resize intent. They
//! carry NO GPU logic. The engine *produces* a [`GraphicData`] and hands it
//! to the host (via `TerminalHost::insert_graphic`); the host uploads it to
//! a texture. The GPU-side `Graphics`/atlas types stay in `sugarloaf`. See
//! `canario/DESIGN.md` §3.6 and §5 Severance 3.

/// Maximum width/height (in pixels) accepted for a decoded graphic.
pub const MAX_GRAPHIC_DIMENSIONS: [usize; 2] = [4096, 4096];

/// Unique identifier for a graphic added to a grid. An id of `0` is a
/// temporary, non-referenceable image (matching kitty's behavior).
#[derive(Eq, PartialEq, Clone, Debug, Copy, Hash, PartialOrd, Ord, Default)]
pub struct GraphicId(pub u64);

impl GraphicId {
    #[inline]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[inline]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Pixel format of a decoded graphic.
#[derive(Eq, PartialEq, Clone, Debug, Copy)]
pub enum ColorType {
    /// 3 bytes per pixel (red, green, blue).
    Rgb,
    /// 4 bytes per pixel (red, green, blue, alpha).
    Rgba,
}

/// A requested render size for a graphic.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum ResizeParameter {
    /// Computed from the original graphic dimensions.
    Auto,
    /// Specified in number of grid cells.
    Cells(u32),
    /// Specified in pixels.
    Pixels(u32),
    /// Specified as a percentage of the window.
    WindowPercent(u32),
}

/// A graphic resize request carried alongside the pixel data.
#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub struct ResizeCommand {
    pub width: ResizeParameter,
    pub height: ResizeParameter,
    pub preserve_aspect_ratio: bool,
}

/// A single decoded graphic read from the PTY.
///
/// This is the renderer-agnostic payload: identity, dimensions, pixels, and
/// resize/display intent. The host turns `pixels` into a texture.
#[derive(Eq, PartialEq, Clone, Debug)]
pub struct GraphicData {
    /// Graphics identifier.
    pub id: GraphicId,
    /// Width, in pixels, of the graphic.
    pub width: usize,
    /// Height, in pixels, of the graphic.
    pub height: usize,
    /// Color type of the pixels.
    pub color_type: ColorType,
    /// Pixel data (`width * height * bytes_per_pixel`).
    pub pixels: Vec<u8>,
    /// Whether the graphic is known to be free of transparent pixels.
    pub is_opaque: bool,
    /// Optional render-size override.
    pub resize: Option<ResizeCommand>,
    /// Display width in pixels (GPU scaling); `None` = original width.
    pub display_width: Option<usize>,
    /// Display height in pixels (GPU scaling); `None` = original height.
    pub display_height: Option<usize>,
    /// Generation counter for cache invalidation (re-transmission with the
    /// same id). The host skips re-upload when this is unchanged.
    pub transmit_time: std::time::Instant,
}

impl GraphicData {
    /// Whether the image may contain transparent pixels. `false` guarantees
    /// it does not.
    #[inline]
    pub fn maybe_transparent(&self) -> bool {
        !self.is_opaque && self.color_type == ColorType::Rgba
    }

    /// Whether every pixel under a region is opaque. A region exceeding the
    /// image bounds is considered not filled.
    pub fn is_filled(&self, x: usize, y: usize, width: usize, height: usize) -> bool {
        if x + width >= self.width || y + height >= self.height {
            return false;
        }
        if !self.maybe_transparent() {
            return true;
        }
        debug_assert!(self.color_type == ColorType::Rgba);
        for offset_y in y..y + height {
            let offset = offset_y * self.width * 4;
            let row = &self.pixels[offset..offset + width * 4];
            if row.chunks_exact(4).any(|pixel| pixel.last() != Some(&255)) {
                return false;
            }
        }
        true
    }

    /// Compute the display dimensions for this graphic without modifying pixels.
    /// Returns (display_width, display_height) in pixels. If no resize is needed,
    /// returns the original dimensions.
    pub fn compute_display_dimensions(
        &self,
        cell_width: usize,
        cell_height: usize,
        view_width: usize,
        view_height: usize,
    ) -> (usize, usize) {
        let resize = match self.resize {
            Some(resize) => resize,
            None => return (self.width, self.height),
        };

        if (resize.width == ResizeParameter::Auto
            && resize.height == ResizeParameter::Auto)
            || self.height == 0
            || self.width == 0
        {
            return (self.width, self.height);
        }

        let mut width = match resize.width {
            ResizeParameter::Auto => 1,
            ResizeParameter::Pixels(n) => n as usize,
            ResizeParameter::Cells(n) => n as usize * cell_width,
            ResizeParameter::WindowPercent(n) => n as usize * view_width / 100,
        };

        let mut height = match resize.height {
            ResizeParameter::Auto => 1,
            ResizeParameter::Pixels(n) => n as usize,
            ResizeParameter::Cells(n) => n as usize * cell_height,
            ResizeParameter::WindowPercent(n) => n as usize * view_height / 100,
        };

        if width == 0 || height == 0 {
            return (self.width, self.height);
        }

        if resize.width == ResizeParameter::Auto {
            width =
                (self.width as f64 * height as f64 / self.height as f64).round() as usize;
        }

        if resize.height == ResizeParameter::Auto {
            height =
                (self.height as f64 * width as f64 / self.width as f64).round() as usize;
        }

        width = std::cmp::min(width, MAX_GRAPHIC_DIMENSIONS[0]);
        height = std::cmp::min(height, MAX_GRAPHIC_DIMENSIONS[1]);

        if resize.preserve_aspect_ratio {
            // Preserve aspect ratio: fit within width x height
            let scale_w = width as f64 / self.width as f64;
            let scale_h = height as f64 / self.height as f64;
            let scale = scale_w.min(scale_h);
            width = (self.width as f64 * scale).round() as usize;
            height = (self.height as f64 * scale).round() as usize;
        }

        (width, height)
    }
}
