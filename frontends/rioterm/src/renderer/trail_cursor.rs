// Alacritty-style smooth cursor motion. While this overlay animates, the
// active panel suppresses its static grid cursor so only the interpolated
// rectangle is visible.

use rio_backend::ansi::CursorShape;
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

/// Per-frame interpolation at 60 FPS. Mirrors Alacritty's
/// `smooth_motion_factor`.
const SMOOTH_MOTION_FACTOR: f32 = 0.20;

/// Motion multiplier for the side opposite the travel direction. Lower
/// values stretch more; 1.0 keeps the rectangle shape rigid.
const SMOOTH_MOTION_SPRING: f32 = 0.80;

/// Maximum stretch relative to the destination cell size.
const SMOOTH_MOTION_MAX_STRETCH_X: f32 = 3.0;
const SMOOTH_MOTION_MAX_STRETCH_Y: f32 = 2.0;

/// Matches Alacritty's cursor rect alpha. The static cursor is suppressed
/// while this overlay is animating.
const TRAIL_ALPHA: f32 = 1.0;
const DEPTH: f32 = 0.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct CursorRect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl CursorRect {
    #[inline]
    fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    #[inline]
    fn for_shape(
        cursor_x: f32,
        cursor_y: f32,
        cell_width: f32,
        cell_height: f32,
        shape: CursorShape,
    ) -> Option<Self> {
        let thickness = (cell_width * 0.15).round().max(1.0);

        match shape {
            CursorShape::Block => {
                Some(Self::new(cursor_x, cursor_y, cell_width, cell_height))
            }
            CursorShape::Beam => {
                Some(Self::new(cursor_x, cursor_y, thickness, cell_height))
            }
            CursorShape::Underline => Some(Self::new(
                cursor_x,
                cursor_y + cell_height - thickness,
                cell_width,
                thickness,
            )),
            CursorShape::Hidden => None,
        }
    }

    #[inline]
    fn visibly_matches(self, other: Self) -> bool {
        self.x.round() == other.x.round()
            && self.y.round() == other.y.round()
            && self.width.round() == other.width.round()
            && self.height.round() == other.height.round()
    }

    #[inline]
    fn interpolate(self, target: Self, factor: f32) -> Self {
        let interp =
            |from: f32, to: f32, factor: f32| from * (1.0 - factor) + to * factor;

        let dx = target.x - self.x;
        let dy = target.y - self.y;

        let x1_factor = factor * if dx < 0.0 { 1.0 } else { SMOOTH_MOTION_SPRING };
        let y1_factor = factor * if dy < 0.0 { 1.0 } else { SMOOTH_MOTION_SPRING };
        let x2_factor = factor * if dx > 0.0 { 1.0 } else { SMOOTH_MOTION_SPRING };
        let y2_factor = factor * if dy > 0.0 { 1.0 } else { SMOOTH_MOTION_SPRING };

        let mut x1 = interp(self.x, target.x, x1_factor);
        let mut y1 = interp(self.y, target.y, y1_factor);
        let mut x2 = interp(self.x + self.width, target.x + target.width, x2_factor);
        let mut y2 = interp(self.y + self.height, target.y + target.height, y2_factor);

        let max_width = target.width * SMOOTH_MOTION_MAX_STRETCH_X;
        let max_height = target.height * SMOOTH_MOTION_MAX_STRETCH_Y;
        let width = (x2 - x1).min(max_width);
        let height = (y2 - y1).min(max_height);

        if dx < 0.0 {
            x1 = x2 - width;
        } else {
            x2 = x1 + width;
        }

        if dy < 0.0 {
            y1 = y2 - height;
        } else {
            y2 = y1 + height;
        }

        Self {
            x: x1,
            y: y1,
            width: x2 - x1,
            height: y2 - y1,
        }
    }
}

pub struct TrailCursor {
    current: Option<CursorRect>,
    target: Option<CursorRect>,
    last_frame: Instant,
    route_id: Option<usize>,
    animating: bool,
}

impl TrailCursor {
    pub fn new() -> Self {
        Self {
            current: None,
            target: None,
            last_frame: Instant::now(),
            route_id: None,
            animating: false,
        }
    }

    pub fn set_route(&mut self, route_id: usize) {
        if self.route_id != Some(route_id) {
            self.route_id = Some(route_id);
            self.current = None;
            self.target = None;
            self.animating = false;
            self.last_frame = Instant::now();
        }
    }

    /// Update the cursor destination. Called once per frame before
    /// `animate()`. The inputs are physical pixels.
    pub fn set_destination(
        &mut self,
        cursor_x: f32,
        cursor_y: f32,
        cell_width: f32,
        cell_height: f32,
        shape: CursorShape,
        visible: bool,
    ) {
        let Some(target) = visible
            .then(|| {
                CursorRect::for_shape(cursor_x, cursor_y, cell_width, cell_height, shape)
            })
            .flatten()
        else {
            self.current = None;
            self.target = None;
            self.animating = false;
            return;
        };

        if self.target == Some(target) {
            return;
        }

        if self.current.is_none() {
            self.current = Some(target);
        } else {
            if !self.animating {
                self.last_frame = Instant::now();
            }
            self.animating = true;
        }

        self.target = Some(target);
    }

    /// Run animation for one frame. Kept with the same signature as the
    /// previous implementation because the caller already has cell metrics.
    pub fn animate(&mut self, _cell_width: f32, _cell_height: f32) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        let Some(target) = self.target else {
            self.animating = false;
            return;
        };

        let Some(current) = self.current else {
            self.current = Some(target);
            self.animating = false;
            return;
        };

        if current.visibly_matches(target) {
            self.current = Some(target);
            self.animating = false;
            return;
        }

        let factor = (SMOOTH_MOTION_FACTOR * dt * 60.0).clamp(0.0, 1.0);
        let next = current.interpolate(target, factor);

        if next.visibly_matches(target) {
            self.current = Some(target);
            self.animating = false;
        } else {
            self.current = Some(next);
            self.animating = true;
        }
    }

    pub fn draw(
        &self,
        sugarloaf: &mut Sugarloaf,
        scale_factor: f32,
        cursor_color: [f32; 4],
    ) {
        if !self.animating || scale_factor <= 0.0 {
            return;
        }

        let Some(rect) = self.current else {
            return;
        };

        let inv = 1.0 / scale_factor;
        let x1 = rect.x * inv;
        let y1 = rect.y * inv;
        let x2 = (rect.x + rect.width) * inv;
        let y2 = (rect.y + rect.height) * inv;
        let mut color = cursor_color;
        color[3] *= TRAIL_ALPHA;

        sugarloaf.triangle(x1, y1, x2, y1, x2, y2, DEPTH, color);
        sugarloaf.triangle(x1, y1, x2, y2, x1, y2, DEPTH, color);
    }

    /// `true` while the animated rectangle has not visibly settled.
    #[inline]
    pub fn is_animating(&self) -> bool {
        self.animating
    }

    /// The animated overlay replaces the grid cursor while it is moving.
    /// Restrict suppression to the route that owns the overlay so inactive
    /// split panels keep their hollow cursors.
    #[inline]
    pub fn hides_static_cursor(&self, route_id: usize) -> bool {
        self.animating && self.route_id == Some(route_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_overlay_hides_static_cursor_only_on_its_route() {
        let mut cursor = TrailCursor::new();
        cursor.set_route(7);
        cursor.set_destination(0.0, 0.0, 10.0, 20.0, CursorShape::Block, true);
        cursor.set_destination(10.0, 0.0, 10.0, 20.0, CursorShape::Block, true);

        assert!(cursor.hides_static_cursor(7));
        assert!(!cursor.hides_static_cursor(8));
    }
}
