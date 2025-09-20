use core::fmt;
use std::error::Error;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use cursor_icon::CursorIcon;

use crate::platform_impl::{PlatformCustomCursor, PlatformCustomCursorSource};

/// The maximum width and height for a cursor when using [`CustomCursor::from_rgba`].
pub const MAX_CURSOR_SIZE: u16 = 2048;

const PIXEL_SIZE: usize = 4;

/// See [`Window::set_cursor()`][crate::window::Window::set_cursor] for more details.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum Cursor {
    Icon(CursorIcon),
    Custom(CustomCursor),
}

impl Default for Cursor {
    fn default() -> Self {
        Self::Icon(CursorIcon::default())
    }
}

impl From<CursorIcon> for Cursor {
    fn from(icon: CursorIcon) -> Self {
        Self::Icon(icon)
    }
}

impl From<CustomCursor> for Cursor {
    fn from(custom: CustomCursor) -> Self {
        Self::Custom(custom)
    }
}

/// Use a custom image as a cursor (mouse pointer).
///
/// Is guaranteed to be cheap to clone.
///
/// ## Platform-specific
///
/// **Web**: Some browsers have limits on cursor sizes usually at 128x128.
///
/// # Example
///
/// ```no_run
/// # use rio_window::event_loop::ActiveEventLoop;
/// # use rio_window::window::Window;
/// # fn scope(event_loop: &ActiveEventLoop, window: &Window) {
/// use rio_window::window::CustomCursor;
///
/// let w = 10;
/// let h = 10;
/// let rgba = vec![255; (w * h * 4) as usize];
///
/// #[cfg(not(target_family = "wasm"))]
/// let source = CustomCursor::from_rgba(rgba, w, h, w / 2, h / 2).unwrap();
///
/// #[cfg(target_family = "wasm")]
/// let source = {
///     use rio_window::platform::web::CustomCursorExtWebSys;
///     CustomCursor::from_url(String::from("http://localhost:3000/cursor.png"), 0, 0)
/// };
///
/// let custom_cursor = event_loop.create_custom_cursor(source);
///
/// window.set_cursor(custom_cursor.clone());
/// # }
/// ```
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CustomCursor {
    /// Platforms should make sure this is cheap to clone.
    pub(crate) inner: PlatformCustomCursor,
}

impl CustomCursor {
    /// Creates a new cursor from an rgba buffer.
    ///
    /// The alpha channel is assumed to be **not** premultiplied.
    pub fn from_rgba(
        rgba: impl Into<Vec<u8>>,
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
    ) -> Result<CustomCursorSource, BadImage> {
        let _span = tracing::debug_span!(
            "rio_window::Cursor::from_rgba",
            width,
            height,
            hotspot_x,
            hotspot_y
        )
        .entered();

        Ok(CustomCursorSource {
            inner: PlatformCustomCursorSource::from_rgba(
                rgba.into(),
                width,
                height,
                hotspot_x,
                hotspot_y,
            )?,
        })
    }
}

/// Source for [`CustomCursor`].
///
/// See [`CustomCursor`] for more details.
#[derive(Debug)]
pub struct CustomCursorSource {
    pub(crate) inner: PlatformCustomCursorSource,
}

/// An error produced when using [`CustomCursor::from_rgba`] with invalid arguments.
#[derive(Debug, Clone)]
pub enum BadImage {
    /// Produced when the image dimensions are larger than [`MAX_CURSOR_SIZE`]. This doesn't
    /// guarantee that the cursor will work, but should avoid many platform and device specific
    /// limits.
    TooLarge { width: u16, height: u16 },
    /// Produced when the length of the `rgba` argument isn't divisible by 4, thus `rgba` can't be
    /// safely interpreted as 32bpp RGBA pixels.
    ByteCountNotDivisibleBy4 { byte_count: usize },
    /// Produced when the number of pixels (`rgba.len() / 4`) isn't equal to `width * height`.
    /// At least one of your arguments is incorrect.
    DimensionsVsPixelCount {
        width: u16,
        height: u16,
        width_x_height: u64,
        pixel_count: u64,
    },
    /// Produced when the hotspot is outside the image bounds
    HotspotOutOfBounds {
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
    },
}

impl fmt::Display for BadImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadImage::TooLarge { width, height } => write!(
                f,
                "The specified dimensions ({width:?}x{height:?}) are too large. The maximum is \
                 {MAX_CURSOR_SIZE:?}x{MAX_CURSOR_SIZE:?}.",
            ),
            BadImage::ByteCountNotDivisibleBy4 { byte_count } => write!(
                f,
                "The length of the `rgba` argument ({byte_count:?}) isn't divisible by 4, making \
                 it impossible to interpret as 32bpp RGBA pixels.",
            ),
            BadImage::DimensionsVsPixelCount { width, height, width_x_height, pixel_count } => {
                write!(
                    f,
                    "The specified dimensions ({width:?}x{height:?}) don't match the number of \
                     pixels supplied by the `rgba` argument ({pixel_count:?}). For those \
                     dimensions, the expected pixel count is {width_x_height:?}.",
                )
            },
            BadImage::HotspotOutOfBounds { width, height, hotspot_x, hotspot_y } => write!(
                f,
                "The specified hotspot ({hotspot_x:?}, {hotspot_y:?}) is outside the image bounds \
                 ({width:?}x{height:?}).",
            ),
        }
    }
}

impl Error for BadImage {}

/// Platforms export this directly as `PlatformCustomCursorSource` if they need to only work with
/// images.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct OnlyCursorImageSource(pub(crate) CursorImage);

#[allow(dead_code)]
impl OnlyCursorImageSource {
    pub(crate) fn from_rgba(
        rgba: Vec<u8>,
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
    ) -> Result<Self, BadImage> {
        CursorImage::from_rgba(rgba, width, height, hotspot_x, hotspot_y).map(Self)
    }
}

/// Platforms export this directly as `PlatformCustomCursor` if they don't implement caching.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct OnlyCursorImage(pub(crate) Arc<CursorImage>);

impl Hash for OnlyCursorImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        Arc::as_ptr(&self.0).hash(state);
    }
}

impl PartialEq for OnlyCursorImage {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for OnlyCursorImage {}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct CursorImage {
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u16,
    pub(crate) height: u16,
    pub(crate) hotspot_x: u16,
    pub(crate) hotspot_y: u16,
}

impl CursorImage {
    pub(crate) fn from_rgba(
        rgba: Vec<u8>,
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
    ) -> Result<Self, BadImage> {
        if width > MAX_CURSOR_SIZE || height > MAX_CURSOR_SIZE {
            return Err(BadImage::TooLarge { width, height });
        }

        if !rgba.len().is_multiple_of(PIXEL_SIZE) {
            return Err(BadImage::ByteCountNotDivisibleBy4 {
                byte_count: rgba.len(),
            });
        }

        let pixel_count = (rgba.len() / PIXEL_SIZE) as u64;
        let width_x_height = width as u64 * height as u64;
        if pixel_count != width_x_height {
            return Err(BadImage::DimensionsVsPixelCount {
                width,
                height,
                width_x_height,
                pixel_count,
            });
        }

        if hotspot_x >= width || hotspot_y >= height {
            return Err(BadImage::HotspotOutOfBounds {
                width,
                height,
                hotspot_x,
                hotspot_y,
            });
        }

        Ok(CursorImage {
            rgba,
            width,
            height,
            hotspot_x,
            hotspot_y,
        })
    }
}

// Platforms that don't support cursors will export this as `PlatformCustomCursor`.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct NoCustomCursor;

#[allow(dead_code)]
impl NoCustomCursor {
    pub(crate) fn from_rgba(
        rgba: Vec<u8>,
        width: u16,
        height: u16,
        hotspot_x: u16,
        hotspot_y: u16,
    ) -> Result<Self, BadImage> {
        CursorImage::from_rgba(rgba, width, height, hotspot_x, hotspot_y)?;
        Ok(Self)
    }
}
