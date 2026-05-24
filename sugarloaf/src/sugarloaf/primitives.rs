// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use serde::Deserialize;

/// Corner radii for a rounded rectangle.
/// Each corner can have a different radius.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
#[repr(C)]
pub struct Corners {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl Corners {
    /// Create corners with the same radius for all corners.
    #[inline]
    pub fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    /// Create corners with zero radius (sharp corners).
    #[inline]
    pub fn zero() -> Self {
        Self::default()
    }

    /// Check if all corners are zero (no rounding).
    #[inline]
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_right == 0.0
            && self.bottom_left == 0.0
    }

    /// Convert to array [top_left, top_right, bottom_right, bottom_left].
    #[inline]
    pub fn to_array(&self) -> [f32; 4] {
        [
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        ]
    }
}

impl From<f32> for Corners {
    fn from(radius: f32) -> Self {
        Self::all(radius)
    }
}

impl From<[f32; 4]> for Corners {
    fn from(arr: [f32; 4]) -> Self {
        Self {
            top_left: arr[0],
            top_right: arr[1],
            bottom_right: arr[2],
            bottom_left: arr[3],
        }
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum CursorKind {
    Block,
    HollowBlock,
    Caret,
    Underline,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct SugarCursor {
    pub kind: CursorKind,
    pub color: [f32; 4],
    pub order: u8,
}

#[derive(Clone, Deserialize, Debug, PartialEq)]
pub struct ImageProperties {
    #[serde(default = "String::default")]
    pub path: String,
    /// Multiplier applied to the image's alpha channel before upload.
    /// Clamped to `[0.0, 1.0]`. `1.0` (the default) means fully opaque;
    /// lower values let the terminal background show through.
    #[serde(default = "default_image_opacity")]
    pub opacity: f32,
}

#[inline]
fn default_image_opacity() -> f32 {
    1.0
}

impl Default for ImageProperties {
    fn default() -> Self {
        Self {
            path: String::new(),
            opacity: default_image_opacity(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub color: [f32; 4],
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> Self {
        Self {
            x,
            y,
            width,
            height,
            color,
        }
    }
}

/// A quad with per-corner radii.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub background_color: [f32; 4],
    pub corner_radii: Corners,
}

impl Quad {
    pub fn new(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        background_color: [f32; 4],
        corner_radii: Corners,
    ) -> Self {
        Self {
            x,
            y,
            width,
            height,
            background_color,
            corner_radii,
        }
    }
}

#[inline]
pub fn is_private_user_area(character: &char) -> bool {
    matches!(
        character,
        '\u{E000}'..='\u{F8FF}'
            | '\u{F0000}'..='\u{FFFFD}'
            | '\u{100000}'..='\u{10FFFD}'
    )
}
