use rio_backend::ansi::CursorShape;
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

// ── Defaults (matching neovide) ─────────────────────────────────────

/// Animation duration for long jumps (seconds).
const ANIMATION_LENGTH: f32 = 0.15;

/// Animation duration for short (≤2 cell horizontal) movements.
const SHORT_ANIMATION_LENGTH: f32 = 0.04;

/// Trail size 0.0–1.0.  1.0 = max stretch (leading edge jumps instantly,
/// trailing edge lags most).
const TRAIL_SIZE: f32 = 1.0;

/// Default cell percentage for bar / underline thickness (neovide: 1/8).
const DEFAULT_CELL_PERCENTAGE: f32 = 1.0 / 8.0;

/// Standard corner offsets (relative to center, in cell-fraction units).
/// Order: top-left, top-right, bottom-right, bottom-left.
const STANDARD_CORNERS: [(f32, f32); 4] =
    [(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)];

/// Depth / draw order for the trail quad (behind regular cursor text).
const DEPTH: f32 = 0.0;
const ORDER: u8 = 10;

// ── Critically-damped spring (matches neovide exactly) ──────────────

#[derive(Clone)]
struct Spring {
    position: f32,
    velocity: f32,
}

impl Spring {
    fn new() -> Self {
        Self {
            position: 0.0,
            velocity: 0.0,
        }
    }

    fn reset(&mut self) {
        self.position = 0.0;
        self.velocity = 0.0;
    }

    /// Advance by variable `dt`. Returns `true` while still moving.
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

// ── Animated corner ─────────────────────────────────────────────────

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

    fn destination(&self, center_x: f32, center_y: f32, cell_w: f32, cell_h: f32) -> (f32, f32) {
        (
            center_x + self.rel_x * cell_w,
            center_y + self.rel_y * cell_h,
        )
    }

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

// ── Shape-aware corner positions (matching neovide) ─────────────────

/// Compute the four corner relative offsets for a given cursor shape.
///
/// - **Block**: full cell.
/// - **Beam** (vertical bar): x squished so the right side collapses to
///   `cell_percentage` width on the left.
/// - **Underline** (horizontal bar): y squished so the top collapses to
///   `cell_percentage` height at the bottom.
fn shape_corners(shape: CursorShape) -> [(f32, f32); 4] {
    let pct = DEFAULT_CELL_PERCENTAGE;
    match shape {
        CursorShape::Block => STANDARD_CORNERS,
        CursorShape::Beam => {
            // Transform x: (x + 0.5) * pct - 0.5
            STANDARD_CORNERS.map(|(x, y)| ((x + 0.5) * pct - 0.5, y))
        }
        CursorShape::Underline => {
            // Transform y: -((-y + 0.5) * pct - 0.5) — bar sits at bottom
            STANDARD_CORNERS.map(|(x, y)| (x, -((-y + 0.5) * pct - 0.5)))
        }
        // Hidden or any other — treat as block (won't be drawn anyway).
        _ => STANDARD_CORNERS,
    }
}

// ── TrailCursor ─────────────────────────────────────────────────────

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
    /// One-shot flag: set when destination changes, consumed in `animate`.
    jumped: bool,
    /// True until the first real destination is set — first frame teleports.
    first_frame: bool,
    /// Last known cursor shape (to detect shape changes).
    prev_shape: CursorShape,
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
            jumped: false,
            first_frame: true,
            prev_shape: CursorShape::Block,
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
        cursor_shape: CursorShape,
    ) {
        // Update corner relative positions when cursor shape changes.
        if cursor_shape != self.prev_shape {
            let offsets = shape_corners(cursor_shape);
            for (i, corner) in self.corners.iter_mut().enumerate() {
                corner.rel_x = offsets[i].0;
                corner.rel_y = offsets[i].1;
            }
            self.prev_shape = cursor_shape;
        }

        // Center of cursor cell.
        let cx = cursor_x + cell_width * 0.5;
        let cy = cursor_y + cell_height * 0.5;
        self.dest_cx = cx;
        self.dest_cy = cy;

        // Detect a jump (destination changed).
        if (cx - self.prev_dest_cx).abs() > 0.01
            || (cy - self.prev_dest_cy).abs() > 0.01
        {
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
        let dt = now
            .duration_since(self.last_frame)
            .as_secs_f32()
            .min(0.1);
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
    fn compute_jump(
        &mut self,
        cx: f32,
        cy: f32,
        cell_width: f32,
        cell_height: f32,
    ) {
        // Compute jump vector in cell units for short-movement detection.
        // Use corner 0's previous destination to measure the jump, same
        // as neovide computing (corner_destination - previous_destination)
        // / cursor_dimensions.
        let (prev_dest_x, prev_dest_y) =
            (self.corners[0].prev_dest_x, self.corners[0].prev_dest_y);
        let (new_dest_x, new_dest_y) =
            self.corners[0].destination(cx, cy, cell_width, cell_height);

        let jump_x = if cell_width > 0.0 {
            ((new_dest_x - prev_dest_x) / cell_width).abs()
        } else {
            0.0
        };
        let jump_y = if cell_height > 0.0 {
            ((new_dest_y - prev_dest_y) / cell_height).abs()
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
            (0, self.corners[0].direction_alignment(cx, cy, cell_width, cell_height)),
            (1, self.corners[1].direction_alignment(cx, cy, cell_width, cell_height)),
            (2, self.corners[2].direction_alignment(cx, cy, cell_width, cell_height)),
            (3, self.corners[3].direction_alignment(cx, cy, cell_width, cell_height)),
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
        let edges: [(usize, usize); 4] = [(0, 1), (1, 2), (2, 3), (3, 0)];

        // Bounding box.
        let min_y = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let max_y = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        let height = max_y - min_y;

        if height < 0.5 {
            return;
        }

        let steps = (height / 1.0).ceil().max(1.0) as usize;
        let step_h = height / steps as f32;

        for s in 0..steps {
            let y = min_y + (s as f32 + 0.5) * step_h;
            let mut x_min = f32::INFINITY;
            let mut x_max = f32::NEG_INFINITY;

            // Intersect this scanline with each edge.
            for &(a, b) in &edges {
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
