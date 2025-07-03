//! Gradient support for sugarloaf, adapted from iced-rs
use bytemuck::{Pod, Zeroable};
use half::f16;
use std::cmp::Ordering;

/// A color stop in a gradient
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct ColorStop {
    /// Offset along the gradient vector (0.0 to 1.0)
    pub offset: f32,
    /// The color at this stop [r, g, b, a]
    pub color: [f32; 4],
}

/// A linear gradient definition
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinearGradient {
    /// Starting point of the gradient
    pub start: [f32; 2],
    /// Ending point of the gradient  
    pub end: [f32; 2],
    /// Color stops (up to 8 supported)
    pub stops: [Option<ColorStop>; 8],
}

impl LinearGradient {
    /// Creates a new linear gradient from start to end point
    pub fn new(start: [f32; 2], end: [f32; 2]) -> Self {
        Self {
            start,
            end,
            stops: [None; 8],
        }
    }

    /// Adds a color stop to the gradient
    pub fn add_stop(mut self, offset: f32, color: [f32; 4]) -> Self {
        if offset.is_finite() && (0.0..=1.0).contains(&offset) {
            let (Ok(index) | Err(index)) =
                self.stops.binary_search_by(|stop| match stop {
                    None => Ordering::Greater,
                    Some(stop) => stop.offset.partial_cmp(&offset).unwrap(),
                });

            if index < 8 {
                self.stops[index] = Some(ColorStop { offset, color });
            }
        }

        self
    }

    /// Adds multiple color stops
    pub fn add_stops(mut self, stops: impl IntoIterator<Item = ColorStop>) -> Self {
        for stop in stops {
            self = self.add_stop(stop.offset, stop.color);
        }
        self
    }

    /// Packs the gradient for use in shaders
    pub fn pack(&self) -> PackedGradient {
        let mut colors = [[0u32; 2]; 8];
        let mut offsets = [f16::from(0u8); 8];

        for (index, stop) in self.stops.iter().enumerate() {
            let color = stop.map_or([0.0, 0.0, 0.0, 0.0], |s| s.color);
            
            colors[index] = [
                pack_f16s([f16::from_f32(color[0]), f16::from_f32(color[1])]),
                pack_f16s([f16::from_f32(color[2]), f16::from_f32(color[3])]),
            ];

            offsets[index] = stop.map_or(f16::from_f32(2.0), |s| f16::from_f32(s.offset));
        }

        let packed_offsets = [
            pack_f16s([offsets[0], offsets[1]]),
            pack_f16s([offsets[2], offsets[3]]),
            pack_f16s([offsets[4], offsets[5]]),
            pack_f16s([offsets[6], offsets[7]]),
        ];

        PackedGradient {
            colors,
            offsets: packed_offsets,
            direction: [self.start[0], self.start[1], self.end[0], self.end[1]],
        }
    }
}

/// Packed gradient data for GPU usage
#[derive(Debug, Copy, Clone, PartialEq, Zeroable, Pod)]
#[repr(C)]
pub struct PackedGradient {
    /// 8 colors, each channel = 16 bit float, 2 colors packed into 1 u32
    pub colors: [[u32; 2]; 8],
    /// 8 offsets, 8x 16 bit floats packed into 4 u32s
    pub offsets: [u32; 4],
    /// Start and end points [start_x, start_y, end_x, end_y]
    pub direction: [f32; 4],
}

impl Default for PackedGradient {
    fn default() -> Self {
        Self {
            colors: [[0; 2]; 8],
            offsets: [0; 4],
            direction: [0.0; 4],
        }
    }
}

/// Packs two f16 values into a single u32
fn pack_f16s(f: [f16; 2]) -> u32 {
    let one = (f[0].to_bits() as u32) << 16;
    let two = f[1].to_bits() as u32;
    one | two
}

/// Convenience function to create a simple two-color gradient
pub fn linear_gradient(
    start: [f32; 2],
    end: [f32; 2],
    start_color: [f32; 4],
    end_color: [f32; 4],
) -> LinearGradient {
    LinearGradient::new(start, end)
        .add_stop(0.0, start_color)
        .add_stop(1.0, end_color)
}

/// Creates a vertical gradient from top to bottom
pub fn vertical_gradient(
    x: f32,
    y: f32,
    _width: f32,
    height: f32,
    start_color: [f32; 4],
    end_color: [f32; 4],
) -> LinearGradient {
    linear_gradient([x, y], [x, y + height], start_color, end_color)
}

/// Creates a horizontal gradient from left to right
pub fn horizontal_gradient(
    x: f32,
    y: f32,
    width: f32,
    _height: f32,
    start_color: [f32; 4],
    end_color: [f32; 4],
) -> LinearGradient {
    linear_gradient([x, y], [x + width, y], start_color, end_color)
}