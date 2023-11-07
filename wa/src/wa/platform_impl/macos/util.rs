use log::trace;
use core_graphics::display::CGDisplay;
use icrate::Foundation::{CGFloat, NSNotFound, NSPoint, NSRange, NSRect, NSUInteger};

use crate::dpi::LogicalPosition;

// Replace with `!` once stable
#[derive(Debug)]
pub enum Never {}

pub const EMPTY_RANGE: NSRange = NSRange {
    location: NSNotFound as NSUInteger,
    length: 0,
};

macro_rules! trace_scope {
    ($s:literal) => {
        let _crate = $crate::wa::platform_impl::platform::util::TraceGuard::new(module_path!(), $s);
    };
}

pub(crate) struct TraceGuard {
    module_path: &'static str,
    called_from_fn: &'static str,
}

impl TraceGuard {
    #[inline]
    pub(crate) fn new(module_path: &'static str, called_from_fn: &'static str) -> Self {
        trace!(target: module_path, "Triggered `{}`", called_from_fn);
        Self {
            module_path,
            called_from_fn,
        }
    }
}

impl Drop for TraceGuard {
    #[inline]
    fn drop(&mut self) {
        trace!(target: self.module_path, "Completed `{}`", self.called_from_fn);
    }
}

// For consistency with other platforms, this will...
// 1. translate the bottom-left window corner into the top-left window corner
// 2. translate the coordinate from a bottom-left origin coordinate system to a top-left one
#[allow(clippy::unnecessary_cast)]
pub fn bottom_left_to_top_left(rect: NSRect) -> f64 {
    CGDisplay::main().pixels_high() as f64 - (rect.origin.y + rect.size.height) as f64
}

/// Converts from winit screen-coordinates to macOS screen-coordinates.
/// Winit: top-left is (0, 0) and y increasing downwards
/// macOS: bottom-left is (0, 0) and y increasing upwards
pub fn window_position(position: LogicalPosition<f64>) -> NSPoint {
    NSPoint::new(
        position.x as CGFloat,
        CGDisplay::main().pixels_high() as CGFloat - position.y as CGFloat,
    )
}
