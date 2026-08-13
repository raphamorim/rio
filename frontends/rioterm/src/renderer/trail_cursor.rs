// Cursor trail: the four corners of one quad chase the cursor rect
// with exponential ease-out, corners on the leading side of the motion
// faster than trailing ones, which is what stretches the quad into a
// smear. The model is kitty's (kitty/cursor_trail.c): the easing
// `1 - 2^(-10 * dt / decay)` composes exactly across frames, so an
// event-driven renderer with irregular in-flight frame timing animates
// the same path a fixed-cadence one would, no sub-stepping required.
// The one gap that is NOT animation time is idle: no frames are
// scheduled while the trail rests, so the first tick of a new flight
// integrates a nominal frame instead of the whole pause (frame_dt).

use rio_backend::ansi::CursorShape;
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

const DEPTH: f32 = 0.0;

/// A corner is settled within half a physical pixel of its target.
const SETTLE_EPSILON_PX: f32 = 0.5;

/// Nominal frame integrated when a flight starts from idle: the gap
/// since the last rendered frame is not travel time.
const IDLE_FRAME_DT: f32 = 1.0 / 60.0;

/// Per-step cap while in flight, so a stalled compositor cannot
/// complete the whole animation in one step.
const MAX_FRAME_DT: f32 = 0.1;

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
        // NaN passes through clamp; a non-finite knob takes its
        // default rather than poisoning every frame's arithmetic.
        let defaults = TrailSettings::default();
        if !self.opacity.is_finite() {
            self.opacity = defaults.opacity;
        }
        if !self.decay_fast.is_finite() {
            self.decay_fast = defaults.decay_fast;
        }
        if !self.decay_slow.is_finite() {
            self.decay_slow = defaults.decay_slow;
        }
        if !self.start_threshold.is_finite() {
            self.start_threshold = defaults.start_threshold;
        }
        self.opacity = self.opacity.clamp(0.0, 1.0);
        self.decay_fast = self.decay_fast.clamp(0.001, 10.0);
        // The slow corner can never beat the fast one.
        self.decay_slow = self.decay_slow.clamp(self.decay_fast, 10.0);
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
    /// The previous tick painted the quad (nonzero alpha): one more
    /// frame is owed after it stops, so the scene repaints without it.
    painted: bool,
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
            painted: false,
            route_id: None,
            first_frame: true,
            settings: settings.sanitized(),
        }
    }

    /// The animation dt for a frame that arrives `raw` seconds after
    /// the previous one. While idle no frames are scheduled, so the
    /// gap since the last render is not travel time: the first tick
    /// of a new flight integrates one nominal frame instead of
    /// completing the whole flight in a single step. Mid-flight,
    /// frames arrive at vsync cadence and the real dt is what keeps
    /// the easing exact, capped so a compositor stall cannot swallow
    /// the animation either.
    #[inline]
    fn frame_dt(&self, raw: f32) -> f32 {
        if self.active {
            raw.min(MAX_FRAME_DT)
        } else {
            IDLE_FRAME_DT
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
        let dt = self.frame_dt(now.duration_since(self.last_tick).as_secs_f32());
        self.last_tick = now;
        let visible = visible && shape != CursorShape::Hidden;

        let rect = target_rect(cursor_x, cursor_y, cell_width, cell_height, shape);
        let fresh_target = rect.is_some();
        if let Some(rect) = rect {
            self.target = rect;
        }

        self.tick(dt, visible, fresh_target, route_id, cell_width, cell_height)
    }

    /// The dt-explicit core of [`update`], separated so tests control
    /// time.
    fn tick(
        &mut self,
        dt: f32,
        visible: bool,
        fresh_target: bool,
        route_id: usize,
        cell_width: f32,
        cell_height: f32,
    ) -> bool {
        // A panel or tab switch is not cursor travel: teleport. A
        // hidden cursor keeps the previous panel's rect, and parking
        // on it would smear from foreign coordinates once the cursor
        // shows, so the teleport waits until a real target exists.
        if self.first_frame || self.route_id != Some(route_id) {
            self.route_id = Some(route_id);
            self.first_frame = !fresh_target;
            self.corners = self.target;
            self.opacity = if visible { 1.0 } else { 0.0 };
            self.moving = false;
            self.was_moving = false;
            self.active = false;
            self.painted = false;
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

        // Visibility ramp. A cursor hidden mid-flight (DECTCEM, as
        // file managers and TUI dashboards do on every refresh) fades
        // its smear out over `decay_slow` instead of letting it
        // animate forever or vanish in one frame. Hidden with the
        // corners parked there is no smear to fade, and a quad
        // lingering over freshly drawn content reads as a phantom
        // cursor, so the opacity drops outright.
        let ramp = dt / self.settings.decay_slow;
        self.opacity = if visible {
            if self.moving || self.was_moving {
                (self.opacity + ramp).min(1.0)
            } else {
                // Parked and visible nothing is drawn, so restore in
                // one step: a cursor that hides and shows per refresh
                // (TUIs, scrollback) must not dim its next flight by
                // ramping up from the last chop.
                1.0
            }
        } else if self.moving || self.was_moving {
            (self.opacity - ramp).max(0.0)
        } else {
            0.0
        };
        if !visible && self.opacity <= 0.0 {
            // Fully faded: park on the target so reappearing starts
            // clean. When the previous frame painted the quad, one
            // more frame is owed so the scene repaints without it.
            self.corners = self.target;
            let extra_frame = self.moving || self.was_moving || self.painted;
            self.moving = false;
            self.was_moving = false;
            self.painted = false;
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
        let result = moving || self.was_moving || fading;
        self.was_moving = moving;
        self.moving = moving;
        self.active = result;
        self.painted = result && self.opacity * self.settings.opacity > 0.0;
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
        // Settled at full opacity the quad is exactly the cursor rect:
        // pure overdraw, and a visible tint when a color override is
        // configured. The extra post-settle frame presents without it.
        if !self.moving && self.opacity >= 1.0 {
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

    /// Adopt the next real target without travel. Layout changes
    /// (window resize, font-size change) displace the cursor without
    /// the cursor having moved; animating that displacement smears
    /// across the reflow.
    #[inline]
    pub fn snap(&mut self) {
        self.first_frame = true;
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
        t.tick(0.016, true, true, 1, CELL_W, CELL_H);
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

        coarse.tick(0.128, true, true, 1, CELL_W, CELL_H);
        for _ in 0..8 {
            fine.tick(0.016, true, true, 1, CELL_W, CELL_H);
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
        t.tick(5.0, true, true, 1, CELL_W, CELL_H);
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
        // At most one extra frame paints the snap, then it goes quiet.
        t.tick(0.016, true, true, 1, CELL_W, CELL_H);
        assert!(!t.tick(0.016, true, true, 1, CELL_W, CELL_H));
    }

    #[test]
    fn settles_within_decay_budget() {
        let mut t = trail(0.0);
        retarget(&mut t, 40.0, 0.0);
        // decay_slow reaches 1/1024 of the distance per its own
        // definition; 2x the budget is comfortably settled.
        let mut frames = 0;
        while t.tick(0.016, true, true, 1, CELL_W, CELL_H) {
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
        assert!(!t.tick(0.016, true, true, 1, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);

        retarget(&mut t, 20.0, 0.0);
        assert!(t.tick(0.016, true, true, 1, CELL_W, CELL_H));
    }

    /// Once in flight, retargets inside the threshold are still
    /// followed: the trail bends with the cursor instead of snapping
    /// mid-animation.
    #[test]
    fn mid_flight_retarget_is_followed() {
        let mut t = trail(2.0);
        retarget(&mut t, 20.0, 0.0);
        assert!(t.tick(0.016, true, true, 1, CELL_W, CELL_H));

        retarget(&mut t, 21.0, 0.0);
        assert!(t.tick(0.016, true, true, 1, CELL_W, CELL_H));
        assert!(max_corner_error(&t) > SETTLE_EPSILON_PX);
    }

    /// Corners on the leading side of the travel catch up faster:
    /// that differential is the smear.
    #[test]
    fn leading_corners_outrun_trailing_ones() {
        let mut t = trail(0.0);
        retarget(&mut t, 20.0, 0.0); // pure rightward travel
        t.tick(0.016, true, true, 1, CELL_W, CELL_H);

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
        t.tick(0.016, true, true, 1, CELL_W, CELL_H);

        // Hide: fade frames are still requested while opacity drains.
        assert!(t.tick(0.016, false, true, 1, CELL_W, CELL_H));
        assert!(t.opacity < 1.0);

        // Drain past decay_slow: parked, quiet.
        t.tick(1.0, false, true, 1, CELL_W, CELL_H);
        t.tick(0.016, false, true, 1, CELL_W, CELL_H);
        assert_eq!(t.opacity, 0.0);
        assert!(!t.tick(0.016, false, true, 1, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
    }

    /// Switching panels or tabs teleports: a route change is not
    /// cursor travel and must not smear across the screen.
    #[test]
    fn route_change_teleports() {
        let mut t = trail(0.0);
        retarget(&mut t, 50.0, 30.0);
        assert!(!t.tick(0.016, true, true, 2, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
    }

    /// The gap since the last rendered frame is idle time, not travel
    /// time: a jump after a pause must animate from one nominal
    /// frame, not complete instantly because dt spanned the pause.
    #[test]
    fn idle_gap_is_not_travel_time() {
        let t = trail(0.0);
        assert!(!t.is_animating());
        assert_eq!(t.frame_dt(2.0), IDLE_FRAME_DT);

        let mut t = trail(0.0);
        retarget(&mut t, 20.0, 0.0);
        assert!(t.tick(0.016, true, true, 1, CELL_W, CELL_H));
        // In flight, real dt applies, capped against stalls.
        assert_eq!(t.frame_dt(0.032), 0.032);
        assert_eq!(t.frame_dt(2.0), MAX_FRAME_DT);
    }

    /// Hiding a cursor whose trail is at rest must not paint a fading
    /// phantom quad over whatever the program draws next.
    #[test]
    fn hide_from_rest_shows_no_phantom() {
        let mut t = trail(0.0);
        // Parked and quiet.
        assert!(!t.tick(0.016, true, true, 1, CELL_W, CELL_H));
        assert!(!t.tick(0.016, false, true, 1, CELL_W, CELL_H));
        assert_eq!(t.opacity, 0.0);
    }

    /// When a mid-flight fade drains out, one final frame is granted
    /// so the scene repaints without the quad; the frame after that
    /// stays quiet (no infinite frame requests).
    #[test]
    fn fade_end_grants_exactly_one_clearing_frame() {
        let mut t = trail(0.0);
        retarget(&mut t, 20.0, 0.0);
        assert!(t.tick(0.016, true, true, 1, CELL_W, CELL_H));

        // Hide and drain the whole fade in one step: the clearing
        // frame is owed because the previous frame painted.
        assert!(t.tick(1.0, false, true, 1, CELL_W, CELL_H));
        assert_eq!(t.opacity, 0.0);
        assert!(!t.tick(0.016, false, true, 1, CELL_W, CELL_H));
    }

    /// A hide/show storm at rest (TUIs redrawing with DECTCEM, or
    /// scrolling back and returning) must leave the trail at full
    /// strength for its next flight, not dimmed by the last chop.
    #[test]
    fn reappearing_at_rest_restores_full_opacity() {
        let mut t = trail(0.0);
        for _ in 0..5 {
            t.tick(0.016, false, true, 1, CELL_W, CELL_H);
            t.tick(0.016, true, true, 1, CELL_W, CELL_H);
        }
        assert_eq!(t.opacity, 1.0);
    }

    /// Corners can settle while a hide-fade still has opacity left:
    /// the drain tick then owes one clearing frame even though both
    /// motion flags already dropped (the motion-flag logic alone
    /// stranded the last painted quad here).
    #[test]
    fn settle_during_fade_still_clears_the_last_quad() {
        let mut t = trail(0.0);
        retarget(&mut t, 20.0, 0.0);
        t.tick(0.016, true, true, 1, CELL_W, CELL_H);

        // Hide mid-flight with a dt that settles the corners while
        // the fade is still in flight.
        assert!(t.tick(0.36, false, true, 1, CELL_W, CELL_H));
        assert!(!t.moving, "corners must have settled");
        assert!(t.opacity > 0.0, "fade must still be in flight");

        // The drain tick paints nothing, but the previous frame did:
        // one clearing frame, then quiet.
        assert!(t.tick(0.05, false, true, 1, CELL_W, CELL_H));
        assert!(!t.tick(0.016, false, true, 1, CELL_W, CELL_H));
    }

    /// A route switch while the cursor is hidden keeps a stale target
    /// from the old panel; the teleport must wait for a real target
    /// so the first visible cursor on the new panel doesn't inherit
    /// foreign coordinates.
    #[test]
    fn route_change_while_hidden_defers_teleport() {
        let mut t = trail(0.0);
        retarget(&mut t, 50.0, 30.0);
        t.tick(0.016, true, true, 1, CELL_W, CELL_H);

        // Switch to route 2 with a hidden cursor: no fresh target.
        assert!(!t.tick(0.016, false, false, 2, CELL_W, CELL_H));

        // The cursor appears on the new panel: teleport, no smear
        // from the old panel's coordinates.
        retarget(&mut t, 0.0, 0.0);
        assert!(!t.tick(0.016, true, true, 2, CELL_W, CELL_H));
        assert!(max_corner_error(&t) <= SETTLE_EPSILON_PX);
    }

    /// After a layout change (`snap`: resize, font-size change), the
    /// next target is adopted without travel even at jump distance.
    #[test]
    fn snap_adopts_next_target_without_travel() {
        let mut t = trail(0.0);
        retarget(&mut t, 40.0, 20.0);
        t.snap();
        assert!(!t.tick(0.016, true, true, 1, CELL_W, CELL_H));
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
