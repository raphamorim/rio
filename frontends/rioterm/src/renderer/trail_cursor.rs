// Cursor trail: the four corners of one quad chase the cursor rect
// with exponential ease-out, corners on the leading side of the motion
// faster than trailing ones, which is what stretches the quad into a
// smear. The model is kitty's (kitty/cursor_trail.c): the easing
// `1 - 2^(-10 * dt / decay)` is exact for any frame gap, so an
// event-driven renderer with irregular frame timing animates the same
// path a fixed-cadence one would, no sub-stepping or frame-rate
// normalization required.

use rio_backend::ansi::CursorShape;
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

const DEPTH: f32 = 0.0;

/// A corner is settled within half a physical pixel of its target.
const SETTLE_EPSILON_PX: f32 = 0.5;

/// Beam and underline thickness as a fraction of the cell width.
const THIN_SHAPE_FRACTION: f32 = 0.15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrailSettings {
    /// Overrides the cursor color when set.
    pub color: Option<[f32; 4]>,
    /// Peak opacity multiplier, 0.0 to 1.0.
    pub opacity: f32,
    /// Catch-up time of the fastest corner, seconds.
    pub decay_fast: f32,
    /// Catch-up time of the slowest corner, seconds. Also the fade
    /// time when the cursor hides.
    pub decay_slow: f32,
    /// Jumps at or under this many cells (both axes) from rest snap
    /// instead of starting a trail.
    pub start_threshold: f32,
}

impl Default for TrailSettings {
    fn default() -> Self {
        Self {
            color: None,
            opacity: 1.0,
            decay_fast: 0.1,
            decay_slow: 0.4,
            start_threshold: 2.0,
        }
    }
}

impl TrailSettings {
    pub fn sanitized(mut self) -> Self {
        self.opacity = self.opacity.clamp(0.0, 1.0);
        self.decay_fast = self.decay_fast.max(0.001);
        // The slow corner can never beat the fast one.
        self.decay_slow = self.decay_slow.max(self.decay_fast);
        self.start_threshold = self.start_threshold.max(0.0);
        self
    }
}

pub struct TrailCursor {
    /// Animated quad corners, TL TR BR BL, physical pixels.
    corners: [[f32; 2]; 4],
    /// Where the corners are headed: the cursor rect for the current
    /// shape. Kept across a hide so the fade-out happens in place.
    target: [[f32; 2]; 4],
    /// Visibility ramp, 0.0 to 1.0. Rises and falls over
    /// `decay_slow` so a hiding cursor fades its trail out instead of
    /// cutting it.
    opacity: f32,
    last_tick: Instant,
    /// Corners still travelling this frame.
    moving: bool,
    /// One extra frame after settling so the final snap paints.
    was_moving: bool,
    /// Frames are still needed (movement or an in-flight fade).
    active: bool,
    route_id: Option<usize>,
    first_frame: bool,
    settings: TrailSettings,
}

/// The cursor rect a shape occupies, as quad corners TL TR BR BL.
/// `None` for a hidden cursor: the trail keeps its previous target
/// and fades where it stands.
fn target_rect(
    x: f32,
    y: f32,
    cell_width: f32,
    cell_height: f32,
    shape: CursorShape,
) -> Option<[[f32; 2]; 4]> {
    let thickness = (cell_width * THIN_SHAPE_FRACTION).round().max(1.0);
    let (x0, y0, w, h) = match shape {
        CursorShape::Block => (x, y, cell_width, cell_height),
        CursorShape::Beam => (x, y, thickness, cell_height),
        CursorShape::Underline => (x, y + cell_height - thickness, cell_width, thickness),
        CursorShape::Hidden => return None,
    };
    Some([[x0, y0], [x0 + w, y0], [x0 + w, y0 + h], [x0, y0 + h]])
}

impl TrailCursor {
    pub fn new(settings: TrailSettings) -> Self {
        Self {
            corners: [[0.0; 2]; 4],
            target: [[0.0; 2]; 4],
            opacity: 0.0,
            last_tick: Instant::now(),
            moving: false,
            was_moving: false,
            active: false,
            route_id: None,
            first_frame: true,
            settings: settings.sanitized(),
        }
    }

    /// Advance the trail toward the cursor at (`cursor_x`,
    /// `cursor_y`) physical pixels. Returns whether another frame is
    /// needed; while it returns `true` the caller must keep frames
    /// coming or the animation freezes mid-flight.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        cursor_x: f32,
        cursor_y: f32,
        cell_width: f32,
        cell_height: f32,
        shape: CursorShape,
        visible: bool,
        route_id: usize,
    ) -> bool {
        let now = Instant::now();
        let dt = now.duration_since(self.last_tick).as_secs_f32();
        self.last_tick = now;
        let visible = visible && shape != CursorShape::Hidden;

        if let Some(rect) =
            target_rect(cursor_x, cursor_y, cell_width, cell_height, shape)
        {
            self.target = rect;
        }

        self.tick(dt, visible, route_id, cell_width, cell_height)
    }

    /// The dt-explicit core of [`update`], separated so tests control
    /// time.
    fn tick(
        &mut self,
        dt: f32,
        visible: bool,
        route_id: usize,
        cell_width: f32,
        cell_height: f32,
    ) -> bool {
        // A panel or tab switch is not cursor travel: teleport.
        if self.first_frame || self.route_id != Some(route_id) {
            self.route_id = Some(route_id);
            self.first_frame = false;
            self.corners = self.target;
            self.opacity = if visible { 1.0 } else { 0.0 };
            self.moving = false;
            self.was_moving = false;
            self.active = false;
            return false;
        }

        // From rest, a jump within the threshold snaps: typing-scale
        // movement stays trail-free. Once in flight, every retarget
        // is followed so the trail bends with the cursor.
        if !self.moving && !self.was_moving {
            let dx = (self.target[0][0] - self.corners[0][0]).abs() / cell_width.max(1.0);
            let dy =
                (self.target[0][1] - self.corners[0][1]).abs() / cell_height.max(1.0);
            if dx <= self.settings.start_threshold && dy <= self.settings.start_threshold
            {
                self.corners = self.target;
            }
        }

        // Visibility ramp. A cursor hidden by the program (DECTCEM,
        // as file managers and TUI dashboards do on every refresh)
        // fades the trail out over `decay_slow` instead of letting it
        // animate forever or vanish in one frame.
        let ramp = dt / self.settings.decay_slow;
        self.opacity = if visible {
            (self.opacity + ramp).min(1.0)
        } else {
            (self.opacity - ramp).max(0.0)
        };
        if !visible && self.opacity <= 0.0 {
            // Fully faded: park on the target so reappearing starts
            // clean.
            self.corners = self.target;
            let extra_frame = self.moving || self.was_moving;
            self.moving = false;
            self.was_moving = false;
            self.active = extra_frame;
            return extra_frame;
        }

        // Per-corner decay by direction alignment: the dot product of
        // each corner's remaining travel with its outward direction
        // from the target center says whether the corner sits on the
        // leading side of the motion. Leading corners take
        // `decay_fast`, trailing ones `decay_slow`, normalized across
        // the four so any travel direction stretches the quad.
        let center_x = (self.target[0][0] + self.target[2][0]) * 0.5;
        let center_y = (self.target[0][1] + self.target[2][1]) * 0.5;
        let half_diag = {
            let dx = self.target[0][0] - center_x;
            let dy = self.target[0][1] - center_y;
            (dx * dx + dy * dy).sqrt().max(1e-6)
        };

        let mut dots = [0.0f32; 4];
        let mut any_travel = false;
        for (i, corner) in self.corners.iter().enumerate() {
            let dx = self.target[i][0] - corner[0];
            let dy = self.target[i][1] - corner[1];
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 1e-6 {
                continue;
            }
            any_travel = true;
            let out_x = self.target[i][0] - center_x;
            let out_y = self.target[i][1] - center_y;
            dots[i] = (dx * out_x + dy * out_y) / (dist * half_diag);
        }

        if any_travel {
            let min_dot = dots.iter().cloned().fold(f32::INFINITY, f32::min);
            let max_dot = dots.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let range = max_dot - min_dot;

            for (i, corner) in self.corners.iter_mut().enumerate() {
                let alignment = if range > 1e-6 {
                    (dots[i] - min_dot) / range
                } else {
                    // Uniform travel (pure translation of a settled
                    // quad ranks all corners equally): everyone slow.
                    0.0
                };
                let decay = self.settings.decay_slow
                    + (self.settings.decay_fast - self.settings.decay_slow) * alignment;
                let step = 1.0 - (2.0f32).powf(-10.0 * dt / decay);
                corner[0] += (self.target[i][0] - corner[0]) * step;
                corner[1] += (self.target[i][1] - corner[1]) * step;
            }
        }

        // Settled corners snap so the quad ends exactly on the cell.
        let mut moving = false;
        for (i, corner) in self.corners.iter_mut().enumerate() {
            let dx = self.target[i][0] - corner[0];
            let dy = self.target[i][1] - corner[1];
            if dx.abs() > SETTLE_EPSILON_PX || dy.abs() > SETTLE_EPSILON_PX {
                moving = true;
            } else {
                *corner = self.target[i];
            }
        }

        let fading = !visible && self.opacity > 0.0;
        let filling = visible && self.opacity < 1.0 && moving;
        let result = moving || self.was_moving || fading || filling;
        self.was_moving = moving;
        self.moving = moving;
        self.active = result;
        result
    }

    /// Draw the trail quad as two triangles through the existing
    /// vertex pipeline. `cursor_color` applies when no color override
    /// is configured; the visibility ramp and the configured opacity
    /// both scale the alpha.
    pub fn draw(
        &self,
        sugarloaf: &mut Sugarloaf,
        scale_factor: f32,
        cursor_color: [f32; 4],
    ) {
        if !self.active {
            return;
        }
        let alpha = self.opacity * self.settings.opacity;
        if alpha <= 0.0 {
            return;
        }

        let base = self.settings.color.unwrap_or(cursor_color);
        let color = [base[0], base[1], base[2], base[3] * alpha];

        // Logical pixels: sugarloaf.triangle scales internally.
        let inv = 1.0 / scale_factor;
        let pts: [(f32, f32); 4] = [
            (self.corners[0][0] * inv, self.corners[0][1] * inv),
            (self.corners[1][0] * inv, self.corners[1][1] * inv),
            (self.corners[2][0] * inv, self.corners[2][1] * inv),
            (self.corners[3][0] * inv, self.corners[3][1] * inv),
        ];

        // Fan from TL: the shared diagonal stays inside the convex
        // hull.
        sugarloaf.triangle(
            pts[0].0, pts[0].1, pts[1].0, pts[1].1, pts[2].0, pts[2].1, DEPTH, color,
        );
        sugarloaf.triangle(
            pts[0].0, pts[0].1, pts[2].0, pts[2].1, pts[3].0, pts[3].1, DEPTH, color,
        );
    }

    /// `true` while another frame is needed to advance the animation.
    #[inline]
    pub fn is_animating(&self) -> bool {
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL_W: f32 = 10.0;
    const CELL_H: f32 = 20.0;

    fn trail(threshold: f32) -> TrailCursor {
        let mut t = TrailCursor::new(TrailSettings {
            start_threshold: threshold,
            ..TrailSettings::default()
        });
        // First tick teleports onto the origin cell.
        t.target = target_rect(0.0, 0.0, CELL_W, CELL_H, CursorShape::Block).unwrap();
        t.tick(0.016, true, 1, CELL_W, CELL_H);
        t
    }

    fn retarget(t: &mut TrailCursor, col: f32, row: f32) {
        t.target = target_rect(
            col * CELL_W,
            row * CELL_H,
            CELL_W,
            CELL_H,
            CursorShape::Block,
        )
        .unwrap();
    }

    fn max_corner_error(t: &TrailCursor) -> f32 {
        t.corners
            .iter()
            .zip(t.target.iter())
            .map(|(c, d)| ((c[0] - d[0]).abs()).max((c[1] - d[1]).abs()))
            .fold(0.0, f32::max)
    }

    /// The exponential form must land on (nearly) the same path
    /// regardless of how the wall-clock time is sliced into frames.
    /// This is the property the previous spring integration lacked,
    /// and it is why event-driven rendering froze or snapped the old
    /// trail. The tolerance is not zero because per-corner decay is
    /// re-ranked from live positions each tick; only that re-ranking
    /// varies with slicing, the easing itself composes exactly.
    #[test]
    fn frame_slicing_does_not_change_the_path() {
        let mut coarse = trail(0.0);
        let mut fine = trail(0.0);
        retarget(&mut coarse, 10.0, 5.0);
        retarget(&mut fine, 10.0, 5.0);

        coarse.tick(0.128, true, 1, CELL_W, CELL_H);
        for _ in 0..8 {
            fine.tick(0.016, true, 1, CELL_W, CELL_H);
        }

        // Travel is ~110px; hold divergence under 2% of it.
        for (a, b) in coarse.corners.iter().zip(fine.corners.iter()) {
            assert!((a[0] - b[0]).abs() < 2.5, "{a:?} vs {b:?}");
            assert!((a[1] - b[1]).abs() < 2.5, "{a:?} vs {b:?}");
        }
    }

    /// Long frame gaps (an idle terminal waking up) must complete the
    /// animation, not freeze it partway.
    #[test]
    fn giant_dt_settles_instead_of_freezing() {
        let mut t = trail(0.0);
        retarget(&mut t, 30.0, 20.0);
        t.tick(5.0, true, 1, CELL_W, CELL_H);
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
        // One extra frame paints the snap, then it goes quiet.
        assert!(t.tick(0.016, true, 1, CELL_W, CELL_H) || !t.is_animating());
        assert!(!t.tick(0.016, true, 1, CELL_W, CELL_H));
    }

    #[test]
    fn settles_within_decay_budget() {
        let mut t = trail(0.0);
        retarget(&mut t, 40.0, 0.0);
        // decay_slow reaches 1/1024 of the distance per its own
        // definition; 2x the budget is comfortably settled.
        let mut frames = 0;
        while t.tick(0.016, true, 1, CELL_W, CELL_H) {
            frames += 1;
            assert!(frames < 100, "did not settle");
        }
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
    }

    /// Typing-scale movement from rest snaps without a trail; jumps
    /// beyond the threshold animate.
    #[test]
    fn start_threshold_gates_from_rest() {
        let mut t = trail(2.0);
        retarget(&mut t, 1.0, 0.0);
        assert!(!t.tick(0.016, true, 1, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);

        retarget(&mut t, 20.0, 0.0);
        assert!(t.tick(0.016, true, 1, CELL_W, CELL_H));
    }

    /// Once in flight, retargets inside the threshold are still
    /// followed: the trail bends with the cursor instead of snapping
    /// mid-animation.
    #[test]
    fn mid_flight_retarget_is_followed() {
        let mut t = trail(2.0);
        retarget(&mut t, 20.0, 0.0);
        assert!(t.tick(0.016, true, 1, CELL_W, CELL_H));

        retarget(&mut t, 21.0, 0.0);
        assert!(t.tick(0.016, true, 1, CELL_W, CELL_H));
        assert!(max_corner_error(&t) > SETTLE_EPSILON_PX);
    }

    /// Corners on the leading side of the travel catch up faster:
    /// that differential is the smear.
    #[test]
    fn leading_corners_outrun_trailing_ones() {
        let mut t = trail(0.0);
        retarget(&mut t, 20.0, 0.0); // pure rightward travel
        t.tick(0.016, true, 1, CELL_W, CELL_H);

        // TR (index 1) leads, TL (index 0) trails.
        let leading_left = t.target[1][0] - t.corners[1][0];
        let trailing_left = t.target[0][0] - t.corners[0][0];
        assert!(
            leading_left < trailing_left,
            "leading {leading_left} vs trailing {trailing_left}"
        );
    }

    /// A hidden cursor fades the trail out and stops the animation;
    /// it must not keep animating during TUI refreshes (yazi,
    /// lazygit) and must not stick at full opacity.
    #[test]
    fn hidden_cursor_fades_out_and_stops() {
        let mut t = trail(0.0);
        retarget(&mut t, 20.0, 0.0);
        t.tick(0.016, true, 1, CELL_W, CELL_H);

        // Hide: fade frames are still requested while opacity drains.
        assert!(t.tick(0.016, false, 1, CELL_W, CELL_H));
        assert!(t.opacity < 1.0);

        // Drain past decay_slow: parked, quiet.
        t.tick(1.0, false, 1, CELL_W, CELL_H);
        t.tick(0.016, false, 1, CELL_W, CELL_H);
        assert_eq!(t.opacity, 0.0);
        assert!(!t.tick(0.016, false, 1, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
    }

    /// Switching panels or tabs teleports: a route change is not
    /// cursor travel and must not smear across the screen.
    #[test]
    fn route_change_teleports() {
        let mut t = trail(0.0);
        retarget(&mut t, 50.0, 30.0);
        assert!(!t.tick(0.016, true, 2, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
    }

    #[test]
    fn settings_sanitize_clamps() {
        let s = TrailSettings {
            color: None,
            opacity: 3.0,
            decay_fast: 0.5,
            decay_slow: 0.1,
            start_threshold: -1.0,
        }
        .sanitized();
        assert_eq!(s.opacity, 1.0);
        assert!(s.decay_slow >= s.decay_fast);
        assert_eq!(s.start_threshold, 0.0);
    }
}
