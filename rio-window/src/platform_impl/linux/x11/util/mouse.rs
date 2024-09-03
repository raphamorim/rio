//! Utilities for handling mouse events.

/// Recorded mouse delta designed to filter out noise.
pub struct Delta<T> {
    x: T,
    y: T,
}

impl<T: Default> Default for Delta<T> {
    fn default() -> Self {
        Self {
            x: Default::default(),
            y: Default::default(),
        }
    }
}

impl<T: Default> Delta<T> {
    pub(crate) fn set_x(&mut self, x: T) {
        self.x = x;
    }

    pub(crate) fn set_y(&mut self, y: T) {
        self.y = y;
    }
}

macro_rules! consume {
    ($this:expr, $ty:ty) => {{
        let this = $this;
        let (x, y) = match (this.x.abs() < <$ty>::EPSILON, this.y.abs() < <$ty>::EPSILON)
        {
            (true, true) => return None,
            (false, true) => (this.x, 0.0),
            (true, false) => (0.0, this.y),
            (false, false) => (this.x, this.y),
        };

        Some((x, y))
    }};
}

impl Delta<f32> {
    pub(crate) fn consume(self) -> Option<(f32, f32)> {
        consume!(self, f32)
    }
}

impl Delta<f64> {
    pub(crate) fn consume(self) -> Option<(f64, f64)> {
        consume!(self, f64)
    }
}
