use crate::platform_impl::PlatformIcon;
use std::error::Error;
use std::{fmt, io, mem};

#[repr(C)]
#[derive(Debug)]
pub(crate) struct Pixel {
    pub(crate) r: u8,
    pub(crate) g: u8,
    pub(crate) b: u8,
    pub(crate) a: u8,
}

pub(crate) const PIXEL_SIZE: usize = mem::size_of::<Pixel>();

#[derive(Debug)]
/// An error produced when using [`Icon::from_rgba`] with invalid arguments.
pub enum BadIcon {
    /// Produced when the length of the `rgba` argument isn't divisible by 4, thus `rgba` can't be
    /// safely interpreted as 32bpp RGBA pixels.
    ByteCountNotDivisibleBy4 { byte_count: usize },
    /// Produced when the number of pixels (`rgba.len() / 4`) isn't equal to `width * height`.
    /// At least one of your arguments is incorrect.
    DimensionsVsPixelCount {
        width: u32,
        height: u32,
        width_x_height: usize,
        pixel_count: usize,
    },
    /// Produced when underlying OS functionality failed to create the icon
    OsError(io::Error),
}

impl fmt::Display for BadIcon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BadIcon::ByteCountNotDivisibleBy4 { byte_count } => write!(
                f,
                "The length of the `rgba` argument ({byte_count:?}) isn't divisible by 4, making \
                 it impossible to interpret as 32bpp RGBA pixels.",
            ),
            BadIcon::DimensionsVsPixelCount { width, height, width_x_height, pixel_count } => {
                write!(
                    f,
                    "The specified dimensions ({width:?}x{height:?}) don't match the number of \
                     pixels supplied by the `rgba` argument ({pixel_count:?}). For those \
                     dimensions, the expected pixel count is {width_x_height:?}.",
                )
            },
            BadIcon::OsError(e) => write!(f, "OS error when instantiating the icon: {e:?}"),
        }
    }
}

impl Error for BadIcon {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RgbaIcon {
    pub(crate) rgba: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// For platforms which don't have window icons (e.g. web)
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoIcon;

#[allow(dead_code)] // These are not used on every platform
mod constructors {
    use super::*;

    impl RgbaIcon {
        pub fn from_rgba(
            rgba: Vec<u8>,
            width: u32,
            height: u32,
        ) -> Result<Self, BadIcon> {
            if !rgba.len().is_multiple_of(PIXEL_SIZE) {
                return Err(BadIcon::ByteCountNotDivisibleBy4 {
                    byte_count: rgba.len(),
                });
            }
            let pixel_count = rgba.len() / PIXEL_SIZE;
            if pixel_count != (width * height) as usize {
                Err(BadIcon::DimensionsVsPixelCount {
                    width,
                    height,
                    width_x_height: (width * height) as usize,
                    pixel_count,
                })
            } else {
                Ok(RgbaIcon {
                    rgba,
                    width,
                    height,
                })
            }
        }
    }

    impl NoIcon {
        pub fn from_rgba(
            rgba: Vec<u8>,
            width: u32,
            height: u32,
        ) -> Result<Self, BadIcon> {
            // Create the rgba icon anyway to validate the input
            let _ = RgbaIcon::from_rgba(rgba, width, height)?;
            Ok(NoIcon)
        }
    }
}

/// An icon used for the window titlebar, taskbar, etc.
#[derive(Clone)]
pub struct Icon {
    pub(crate) inner: PlatformIcon,
}

impl fmt::Debug for Icon {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        fmt::Debug::fmt(&self.inner, formatter)
    }
}

impl Icon {
    /// Creates an icon from 32bpp RGBA data.
    ///
    /// The length of `rgba` must be divisible by 4, and `width * height` must equal
    /// `rgba.len() / 4`. Otherwise, this will return a `BadIcon` error.
    pub fn from_rgba(rgba: Vec<u8>, width: u32, height: u32) -> Result<Self, BadIcon> {
        let _span =
            tracing::debug_span!("rio_window::Icon::from_rgba", width, height).entered();

        Ok(Icon {
            inner: PlatformIcon::from_rgba(rgba, width, height)?,
        })
    }
}
