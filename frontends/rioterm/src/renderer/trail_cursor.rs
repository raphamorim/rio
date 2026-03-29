// This file was heavily inspired by neovide implementation.

use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

/// Animation duration for long jumps (seconds).
const ANIMATION_LENGTH: f32 = 0.15;

/// Animation duration for short (≤2 cell horizontal) movements.
const SHORT_ANIMATION_LENGTH: f32 = 0.04;

/// Trail size 0.0–1.0.  1.0 = max stretch (leading edge jumps instantly,
/// trailing edge lags most).
const TRAIL_SIZE: f32 = 1.0;

/// Depth / draw order for the trail quad (behind regular cursor text).
const DEPTH: f32 = 0.0;
const ORDER: u8 = 10;

#[derive(Clone)]
struct Spring {
    position: f32,
    velocity: f32,
}

impl Spring {
    #[inline]
    fn new() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
        }
    }

    #[inline]
    fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
    }

    /// Advance by variable `dt`. Returns `true` while still moving.
    #[inline]
    fn update(&mut self, dt: f32, animation_length: f32) -> bool {
        if animation_length <= dt {
            self.reset();
            return false;
        }
        if self.position == 0.0 {
            return false;
        }

        // Critically-damped spring (zeta = 1.0).
        // omega chosen so destination is reached within ~2% tolerance in
        // `animation_length` time.
        let omega = 4.0 / animation_length;

        // Analytical solution for critically-damped harmonic oscillation.
        let a = self.position;
        let b = a * omega + self.velocity;
        let c = (-omega * dt).exp();

        self.position = (a + b * dt) * c;
        self.velocity = c * (-a * omega - b * dt * omega + b);

        if self.position.abs() < 0.01 {
            self.reset();
            false
        } else {
            true
        }
    }
}

#[derive(Clone)]
struct Corner {
    spring_x: Spring,
    spring_y: Spring,
    /// Current animated pixel position.
    x: f32,
    y: f32,
    /// Offset relative to cursor center (shape-aware).
    rel_x: f32,
    rel_y: f32,
    prev_dest_x: f32,
    prev_dest_y: f32,
    anim_length: f32,
}

impl Corner {
    fn new(rel_x: f32, rel_y: f32) -> Self {
        Self {
            spring_x: Spring::new(),
            spring_y: Spring::new(),
            x: 0.0,
            y: 0.0,
            rel_x,
            rel_y,
            prev_dest_x: -1e6,
            prev_dest_y: -1e6,
            anim_length: 0.0,
        }
    }

    #[inline]
    fn destination(
        &self,
        center_x: f32,
        center_y: f32,
        cell_w: f32,
        cell_h: f32,
    ) -> (f32, f32) {
        (
            center_x + self.rel_x * cell_w,
            center_y + self.rel_y * cell_h,
        )
    }

    #[inline]
    fn update(
        &mut self,
        center_x: f32,
        center_y: f32,
        cell_w: f32,
        cell_h: f32,
        dt: f32,
        immediate_movement: bool,
    ) -> bool {
        let (dest_x, dest_y) = self.destination(center_x, center_y, cell_w, cell_h);

        if (dest_x - self.prev_dest_x).abs() > 0.01
            || (dest_y - self.prev_dest_y).abs() > 0.01
        {
            self.spring_x.position = dest_x - self.x;
            self.spring_y.position = dest_y - self.y;
            self.prev_dest_x = dest_x;
            self.prev_dest_y = dest_y;
        }

        // Teleport: snap to destination without animating.
        if immediate_movement {
            self.x = dest_x;
            self.y = dest_y;
            self.spring_x.reset();
            self.spring_y.reset();
            return false;
        }

        let mut animating = self.spring_x.update(dt, self.anim_length);
        animating |= self.spring_y.update(dt, self.anim_length);
        self.x = dest_x - self.spring_x.position;
        self.y = dest_y - self.spring_y.position;

        animating
    }

    /// Direction alignment: dot product of the corner's relative direction
    /// with the travel direction.  Higher = more aligned with movement =
    /// "leading".  Matches neovide's `calculate_direction_alignment`.
    #[inline]
    fn direction_alignment(
        &self,
        center_x: f32,
        center_y: f32,
        cell_w: f32,
        cell_h: f32,
    ) -> f32 {
        let (dest_x, dest_y) = self.destination(center_x, center_y, cell_w, cell_h);

        // Corner's relative direction (normalized).
        let rel_len = (self.rel_x * self.rel_x + self.rel_y * self.rel_y)
            .sqrt()
            .max(1e-6);
        let corner_dir_x = self.rel_x / rel_len;
        let corner_dir_y = self.rel_y / rel_len;

        // Travel direction (from current animated pos to destination).
        let dx = dest_x - self.x;
        let dy = dest_y - self.y;
        let travel_len = (dx * dx + dy * dy).sqrt().max(1e-6);

        (dx / travel_len) * corner_dir_x + (dy / travel_len) * corner_dir_y
    }
}

pub struct TrailCursor {
    /// Four corners: [top-left, top-right, bottom-right, bottom-left].
    corners: [Corner; 4],
    last_frame: Instant,
    /// Current destination center (physical pixels).
    dest_cx: f32,
    dest_cy: f32,
    /// Previous destination center, used to detect jumps.
    prev_dest_cx: f32,
    prev_dest_cy: f32,
    /// Center before the current jump — preserved so `compute_jump` can
    /// measure travel distance (since `set_destination` overwrites
    /// `prev_dest` before `animate` runs).
    jump_from_cx: f32,
    jump_from_cy: f32,
    /// One-shot flag: set when destination changes, consumed in `animate`.
    jumped: bool,
    /// True until the first real destination is set — first frame teleports.
    first_frame: bool,
    animating: bool,
}

impl TrailCursor {
    pub fn new() -> Self {
        Self {
            corners: [
                Corner::new(-0.5, -0.5), // top-left
                Corner::new(0.5, -0.5),  // top-right
                Corner::new(0.5, 0.5),   // bottom-right
                Corner::new(-0.5, 0.5),  // bottom-left
            ],
            last_frame: Instant::now(),
            dest_cx: 0.0,
            dest_cy: 0.0,
            prev_dest_cx: -1e6,
            prev_dest_cy: -1e6,
            jump_from_cx: -1e6,
            jump_from_cy: -1e6,
            jumped: false,
            first_frame: true,
            animating: false,
        }
    }

    /// Update the cursor destination.  Called once per frame **before**
    /// `animate()`.  Sets the `jumped` flag when the destination changes
    /// (matching neovide's `update_cursor_destination`).
    pub fn set_destination(
        &mut self,
        cursor_x: f32,
        cursor_y: f32,
        cell_width: f32,
        cell_height: f32,
    ) {
        // Center of cursor cell.
        let cx = cursor_x + cell_width * 0.5;
        let cy = cursor_y + cell_height * 0.5;
        self.dest_cx = cx;
        self.dest_cy = cy;

        // Detect a jump (destination changed).
        if (cx - self.prev_dest_cx).abs() > 0.01 || (cy - self.prev_dest_cy).abs() > 0.01
        {
            self.jump_from_cx = self.prev_dest_cx;
            self.jump_from_cy = self.prev_dest_cy;
            self.prev_dest_cx = cx;
            self.prev_dest_cy = cy;
            self.jumped = true;
        }
    }

    /// Run animation for one frame.  Called once per frame **after**
    /// `set_destination()`.  If `jumped` is set, computes corner ranking
    /// and assigns animation lengths exactly once per jump (matching
    /// neovide's `animate`).
    pub fn animate(&mut self, cell_width: f32, cell_height: f32) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        let cx = self.dest_cx;
        let cy = self.dest_cy;

        // First frame: teleport all corners to destination without
        // animation (matches neovide's `immediate_movement`).
        let immediate = self.first_frame;
        if self.first_frame {
            self.first_frame = false;
        }

        // On jump: compute ranking and set animation lengths (one-shot).
        if self.jumped && !immediate {
            self.compute_jump(cx, cy, cell_width, cell_height);
        }
        self.jumped = false;

        // Spring update every frame (matching neovide).
        let mut still_animating = false;
        for corner in &mut self.corners {
            if corner.update(cx, cy, cell_width, cell_height, dt, immediate) {
                still_animating = true;
            }
        }

        self.animating = still_animating;
    }

    /// Compute corner direction-alignment ranking and assign animation
    /// lengths.  Called exactly once per cursor jump (matching neovide's
    /// `Corner::jump` called from the `if self.jumped` block).
    fn compute_jump(&mut self, cx: f32, cy: f32, cell_width: f32, cell_height: f32) {
        // Compute jump vector in cell units for short-movement detection.
        // `jump_from` is the center *before* this jump was detected.
        let jump_x = if cell_width > 0.0 {
            ((cx - self.jump_from_cx) / cell_width).abs()
        } else {
            0.0
        };
        let jump_y = if cell_height > 0.0 {
            ((cy - self.jump_from_cy) / cell_height).abs()
        } else {
            0.0
        };
        let is_short = jump_x <= 2.001 && jump_y < 0.001;

        if is_short {
            let t = ANIMATION_LENGTH.min(SHORT_ANIMATION_LENGTH);
            for c in &mut self.corners {
                c.anim_length = t;
            }
            return;
        }

        // Direction-alignment ranking (neovide-style).
        let mut alignments: [(usize, f32); 4] = [
            (
                0,
                self.corners[0].direction_alignment(cx, cy, cell_width, cell_height),
            ),
            (
                1,
                self.corners[1].direction_alignment(cx, cy, cell_width, cell_height),
            ),
            (
                2,
                self.corners[2].direction_alignment(cx, cy, cell_width, cell_height),
            ),
            (
                3,
                self.corners[3].direction_alignment(cx, cy, cell_width, cell_height),
            ),
        ];

        // Sort ascending: lowest alignment = most trailing.
        alignments.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });

        // Build per-corner rank array.
        let mut ranks = [0usize; 4];
        for (rank, &(corner_idx, _)) in alignments.iter().enumerate() {
            ranks[corner_idx] = rank;
        }

        let leading = ANIMATION_LENGTH * (1.0 - TRAIL_SIZE).clamp(0.0, 1.0);
        let trailing = ANIMATION_LENGTH;
        let mid = (leading + trailing) / 2.0;

        for (i, corner) in self.corners.iter_mut().enumerate() {
            corner.anim_length = match ranks[i] {
                0 => trailing,
                1 => mid,
                _ => leading,
            };
        }
    }

    /// Draw the cursor trail as a filled convex quad between the four
    /// animated corners.  We scanline-fill by intersecting each row with
    /// all four edges of the polygon (TL→TR→BR→BL), taking the min/max X.
    /// This handles diagonal movement correctly unlike the old left/right
    /// edge assumption.
    pub fn draw(
        &self,
        sugarloaf: &mut Sugarloaf,
        scale_factor: f32,
        cursor_color: [f32; 4],
    ) {
        if !self.animating {
            return;
        }

        let inv = 1.0 / scale_factor;

        // Corner positions: ordered TL, TR, BR, BL.
        let pts: [(f32, f32); 4] = [
            (self.corners[0].x, self.corners[0].y),
            (self.corners[1].x, self.corners[1].y),
            (self.corners[2].x, self.corners[2].y),
            (self.corners[3].x, self.corners[3].y),
        ];

        // Four edges of the quad: TL→TR, TR→BR, BR→BL, BL→TL.
        const EDGES: [(usize, usize); 4] = [(0, 1), (1, 2), (2, 3), (3, 0)];

        // Bounding box.
        let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let height = max_y - min_y;

        if height < 0.5 {
            return;
        }

        let steps = (height as usize).clamp(1, 640);
        let step_h = height / steps as f32;

        for s in 0..steps {
            let y = min_y + (s as f32 + 0.5) * step_h;
            let mut x_min = f32::INFINITY;
            let mut x_max = f32::NEG_INFINITY;

            // Intersect this scanline with each edge.
            for &(a, b) in &EDGES {
                let (ax, ay) = pts[a];
                let (bx, by) = pts[b];

                // Check if scanline Y is within this edge's Y range.
                let (lo_y, hi_y) = if ay < by { (ay, by) } else { (by, ay) };
                if y < lo_y || y > hi_y || (hi_y - lo_y).abs() < 1e-6 {
                    continue;
                }

                // Lerp X at this Y.
                let t = (y - ay) / (by - ay);
                let x = ax + (bx - ax) * t;
                x_min = x_min.min(x);
                x_max = x_max.max(x);
            }

            if x_max - x_min < 0.1 {
                continue;
            }

            sugarloaf.rect(
                None,
                x_min * inv,
                (min_y + s as f32 * step_h) * inv,
                (x_max - x_min) * inv,
                step_h * inv,
                cursor_color,
                DEPTH,
                ORDER,
            );
        }
    }

    /// `true` while the spring corners haven't settled.
    #[inline]
    pub fn is_animating(&self) -> bool {
        self.animating
    }
}
