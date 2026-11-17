// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Glyph Protocol `fmt=path` payloads: SVG path data sent as text.
//!
//! A payload is one or more layers separated by `|`. Each layer is a
//! header, `:`, then SVG 2 path data:
//!
//! ```text
//! fill=nonzero,fg:M4 4h16v16H4Z|stroke=2,#e5534bff:M6 12h12
//! ```
//!
//! - The header is a paint (`fill=nonzero`, `fill=evenodd`, or
//!   `stroke=<width>`) and a colour (`fg`, `fg@<alpha 0..1>`,
//!   `#rrggbb` or `#rrggbbaa`). `fg` is the cell's foreground colour
//!   when the glyph is drawn. Strokes always have round caps and joins.
//! - Coordinates are in the registration's viewBox (`vb=x,y,w,h`),
//!   y down, as in SVG.
//!
//! Parsing happens once, at registration, and is strict so every
//! terminal draws the same thing: the payload is printable ASCII, the
//! path data follows the SVG 2 grammar (exponents, packed arc flags
//! and implicit command repetition included), and any error rejects
//! the whole payload instead of drawing up to it. Numbers are parsed
//! without regard to locale. Arcs are expanded into cubics here, so
//! the renderer only sees lines, quadratics and cubics.
//!
//! Limits keep drawing cheap. Every point, control points included,
//! must lie within [`MAX_EXTENT_MULTIPLE`] viewBox sizes of the
//! viewBox: geometry far outside the cell is what makes a rasterizer
//! slow. A registration holds at most [`MAX_SEGMENTS`] segments,
//! counted after arcs are expanded, and [`MAX_LAYERS`] layers.

/// Most drawing segments one registration may hold, counted after arcs
/// are expanded into cubics.
pub const MAX_SEGMENTS: usize = 4096;

/// Most layers one registration may hold.
pub const MAX_LAYERS: usize = 16;

/// How far outside the viewBox a point may lie, in viewBox sizes (the
/// larger of its width and height).
pub const MAX_EXTENT_MULTIPLE: f32 = 32.0;

/// Largest magnitude a viewBox value may have. Keeps the coordinate
/// bound, `MAX_EXTENT_MULTIPLE` times the viewBox size, well inside
/// f32's exact-integer range.
pub const MAX_VIEW_BOX_VALUE: f32 = 1_000_000.0;

/// The registration's coordinate system: the rectangle that maps onto
/// the glyph's render span.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

/// How a layer's path is painted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayerPaint {
    Fill(FillRule),
    /// Width in viewBox units; caps and joins are round.
    Stroke {
        width: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerColor {
    /// The cell's foreground colour, with this alpha on top of its own.
    Foreground { alpha: u8 },
    /// Straight (not premultiplied) sRGB.
    Rgba([u8; 4]),
}

/// One drawing command in viewBox coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathLayer {
    pub paint: LayerPaint,
    pub color: LayerColor,
    pub segments: Vec<Segment>,
}

/// A validated `fmt=path` registration.
#[derive(Debug, Clone, PartialEq)]
pub struct PathGlyph {
    pub view_box: ViewBox,
    pub layers: Vec<PathLayer>,
}

impl PathGlyph {
    /// Every layer is drawn in the foreground colour, so the glyph can
    /// be kept as an alpha mask and tinted at draw time, like `glyf`.
    pub fn is_monochrome(&self) -> bool {
        self.layers
            .iter()
            .all(|layer| matches!(layer.color, LayerColor::Foreground { .. }))
    }

    /// Some layer is drawn in the foreground colour.
    pub fn uses_foreground(&self) -> bool {
        self.layers
            .iter()
            .any(|layer| matches!(layer.color, LayerColor::Foreground { .. }))
    }
}

/// Why a payload was rejected. The wire parser maps these onto the
/// register errors: `malformed_payload` and `outline_too_large`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    Malformed,
    TooLarge,
}

/// Parse a `vb=x,y,w,h` value. Separators are commas or spaces, as in
/// SVG's `viewBox`. Width and height must be positive.
pub fn parse_view_box(raw: &[u8]) -> Option<ViewBox> {
    let mut cursor = Cursor { s: raw, pos: 0 };
    cursor.skip_wsp();
    let mut values = [0f32; 4];
    for (i, value) in values.iter_mut().enumerate() {
        if i > 0 {
            cursor.skip_comma_wsp();
        }
        *value = cursor.number().ok()?;
        if value.abs() > MAX_VIEW_BOX_VALUE {
            return None;
        }
    }
    cursor.skip_wsp();
    if !cursor.eof() {
        return None;
    }
    let [x, y, w, h] = values;
    (w > 0.0 && h > 0.0).then_some(ViewBox { x, y, w, h })
}

/// Parse and validate a payload against its viewBox.
pub fn parse(payload: &[u8], view_box: ViewBox) -> Result<PathGlyph, PathError> {
    if payload.is_empty() || !payload.iter().all(|b| (0x20..=0x7E).contains(b)) {
        return Err(PathError::Malformed);
    }
    let bounds = Bounds::new(view_box);
    let mut segment_budget = MAX_SEGMENTS;
    let mut layers = Vec::new();
    for raw_layer in payload.split(|&b| b == b'|') {
        if layers.len() == MAX_LAYERS {
            return Err(PathError::TooLarge);
        }
        let colon = raw_layer
            .iter()
            .position(|&b| b == b':')
            .ok_or(PathError::Malformed)?;
        let (paint, color) = parse_header(&raw_layer[..colon], view_box)?;
        let segments =
            parse_path_data(&raw_layer[colon + 1..], &bounds, &mut segment_budget)?;
        layers.push(PathLayer {
            paint,
            color,
            segments,
        });
    }
    Ok(PathGlyph { view_box, layers })
}

fn parse_header(
    raw: &[u8],
    view_box: ViewBox,
) -> Result<(LayerPaint, LayerColor), PathError> {
    let raw = trim(raw);
    let comma = raw
        .iter()
        .position(|&b| b == b',')
        .ok_or(PathError::Malformed)?;
    let (paint_raw, color_raw) = (trim(&raw[..comma]), trim(&raw[comma + 1..]));

    let paint = match paint_raw {
        b"fill=nonzero" => LayerPaint::Fill(FillRule::NonZero),
        b"fill=evenodd" => LayerPaint::Fill(FillRule::EvenOdd),
        _ => {
            let width_raw = paint_raw
                .strip_prefix(b"stroke=")
                .ok_or(PathError::Malformed)?;
            let width = parse_whole_number(width_raw)?;
            // A stroke wider than the viewBox only costs time: it covers
            // the whole span either way.
            if !(width > 0.0 && width <= view_box.w.max(view_box.h)) {
                return Err(PathError::Malformed);
            }
            LayerPaint::Stroke { width }
        }
    };

    let color = if color_raw == b"fg" {
        LayerColor::Foreground { alpha: 0xFF }
    } else if let Some(alpha_raw) = color_raw.strip_prefix(b"fg@") {
        let alpha = parse_whole_number(alpha_raw)?;
        if !(0.0..=1.0).contains(&alpha) {
            return Err(PathError::Malformed);
        }
        LayerColor::Foreground {
            alpha: (alpha * 255.0).round() as u8,
        }
    } else if let Some(hex) = color_raw.strip_prefix(b"#") {
        LayerColor::Rgba(parse_hex_color(hex).ok_or(PathError::Malformed)?)
    } else {
        return Err(PathError::Malformed);
    };
    Ok((paint, color))
}

fn parse_hex_color(hex: &[u8]) -> Option<[u8; 4]> {
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    let mut out = [0, 0, 0, 0xFF];
    for (i, pair) in hex.chunks(2).enumerate() {
        let s = std::str::from_utf8(pair).ok()?;
        out[i] = u8::from_str_radix(s, 16).ok()?;
    }
    Some(out)
}

/// A number that must fill `raw` entirely.
fn parse_whole_number(raw: &[u8]) -> Result<f32, PathError> {
    let mut cursor = Cursor { s: raw, pos: 0 };
    let value = cursor.number()?;
    if cursor.eof() {
        Ok(value)
    } else {
        Err(PathError::Malformed)
    }
}

fn trim(raw: &[u8]) -> &[u8] {
    let start = raw.iter().position(|&b| b != b' ').unwrap_or(raw.len());
    let end = raw
        .iter()
        .rposition(|&b| b != b' ')
        .map_or(start, |i| i + 1);
    &raw[start..end]
}

/// The box every point must stay in.
struct Bounds {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
}

impl Bounds {
    fn new(vb: ViewBox) -> Self {
        let margin = vb.w.max(vb.h) * MAX_EXTENT_MULTIPLE;
        Self {
            x0: vb.x - margin,
            y0: vb.y - margin,
            x1: vb.x + vb.w + margin,
            y1: vb.y + vb.h + margin,
        }
    }

    fn check(&self, x: f64, y: f64) -> Result<(f32, f32), PathError> {
        let (x, y) = (x as f32, y as f32);
        if x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1 {
            Ok((x, y))
        } else {
            // NaN fails the comparisons and lands here too.
            Err(PathError::TooLarge)
        }
    }
}

struct Cursor<'a> {
    s: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn eof(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    /// The payload is printable ASCII, so space is the only whitespace.
    fn skip_wsp(&mut self) {
        while self.peek() == Some(b' ') {
            self.pos += 1;
        }
    }

    /// SVG's `comma_wsp`, optional: spaces, at most one comma, spaces.
    fn skip_comma_wsp(&mut self) {
        self.skip_wsp();
        if self.peek() == Some(b',') {
            self.pos += 1;
            self.skip_wsp();
        }
    }

    /// After a complete argument set: is another one coming? A comma
    /// commits to it.
    fn more_args(&mut self) -> bool {
        self.skip_wsp();
        match self.peek() {
            Some(b',') => {
                self.pos += 1;
                self.skip_wsp();
                true
            }
            Some(b'0'..=b'9' | b'.' | b'+' | b'-') => true,
            _ => false,
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.pos;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        self.pos - start
    }

    /// SVG 2 `number`: sign, then `digit+`, `digit* "." digit+`, or
    /// `digit+ "." digit+`, then an optional exponent. A trailing `.`
    /// with no digits after it is not part of the number. Parsed with
    /// Rust's float parser, which never consults the locale.
    fn number(&mut self) -> Result<f32, PathError> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        let int_digits = self.digits();
        let mut frac_digits = 0;
        if self.peek() == Some(b'.') {
            let dot = self.pos;
            self.pos += 1;
            frac_digits = self.digits();
            if frac_digits == 0 {
                self.pos = dot;
            }
        }
        if int_digits == 0 && frac_digits == 0 {
            return Err(PathError::Malformed);
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let mark = self.pos;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            if self.digits() == 0 {
                self.pos = mark;
            }
        }
        // Only ASCII digits, signs, `.` and `e` were consumed.
        let text = std::str::from_utf8(&self.s[start..self.pos])
            .map_err(|_| PathError::Malformed)?;
        let value: f64 = text.parse().map_err(|_| PathError::Malformed)?;
        let value = value as f32;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(PathError::TooLarge)
        }
    }

    fn flag(&mut self) -> Result<bool, PathError> {
        let flag = match self.peek() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(PathError::Malformed),
        };
        self.pos += 1;
        Ok(flag)
    }

    fn pair(&mut self) -> Result<(f64, f64), PathError> {
        let x = self.number()? as f64;
        self.skip_comma_wsp();
        let y = self.number()? as f64;
        Ok((x, y))
    }
}

/// The control point a following `S` or `T` reflects.
#[derive(Clone, Copy)]
enum LastControl {
    None,
    Cubic(f64, f64),
    Quad(f64, f64),
}

struct PathWriter<'b> {
    segments: Vec<Segment>,
    bounds: &'b Bounds,
    budget: &'b mut usize,
    /// A `Z` closed the subpath and no `M` has opened the next one yet.
    needs_move: bool,
    start: (f64, f64),
    drew: bool,
}

impl PathWriter<'_> {
    fn spend(&mut self) -> Result<(), PathError> {
        *self.budget = self.budget.checked_sub(1).ok_or(PathError::TooLarge)?;
        self.drew = true;
        Ok(())
    }

    fn move_to(&mut self, x: f64, y: f64) -> Result<(), PathError> {
        let (x, y) = self.bounds.check(x, y)?;
        self.segments.push(Segment::MoveTo(x, y));
        self.start = (x as f64, y as f64);
        self.needs_move = false;
        Ok(())
    }

    /// A drawing command right after `Z` starts a new subpath at the
    /// closed one's start point, as SVG specifies.
    fn ensure_open(&mut self) -> Result<(), PathError> {
        if self.needs_move {
            let (x, y) = self.start;
            self.move_to(x, y)?;
        }
        Ok(())
    }

    fn line_to(&mut self, x: f64, y: f64) -> Result<(), PathError> {
        self.ensure_open()?;
        self.spend()?;
        let (x, y) = self.bounds.check(x, y)?;
        self.segments.push(Segment::LineTo(x, y));
        Ok(())
    }

    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> Result<(), PathError> {
        self.ensure_open()?;
        self.spend()?;
        let (cx, cy) = self.bounds.check(cx, cy)?;
        let (x, y) = self.bounds.check(x, y)?;
        self.segments.push(Segment::QuadTo(cx, cy, x, y));
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn cubic_to(
        &mut self,
        c1x: f64,
        c1y: f64,
        c2x: f64,
        c2y: f64,
        x: f64,
        y: f64,
    ) -> Result<(), PathError> {
        self.ensure_open()?;
        self.spend()?;
        let (c1x, c1y) = self.bounds.check(c1x, c1y)?;
        let (c2x, c2y) = self.bounds.check(c2x, c2y)?;
        let (x, y) = self.bounds.check(x, y)?;
        self.segments
            .push(Segment::CubicTo(c1x, c1y, c2x, c2y, x, y));
        Ok(())
    }

    fn close(&mut self) {
        if !self.needs_move {
            self.segments.push(Segment::Close);
            self.needs_move = true;
        }
    }
}

fn parse_path_data(
    raw: &[u8],
    bounds: &Bounds,
    budget: &mut usize,
) -> Result<Vec<Segment>, PathError> {
    let mut c = Cursor { s: raw, pos: 0 };
    let mut out = PathWriter {
        segments: Vec::new(),
        bounds,
        budget,
        needs_move: false,
        start: (0.0, 0.0),
        drew: false,
    };
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    let mut last = LastControl::None;

    c.skip_wsp();
    // Path data starts with a moveto.
    if !matches!(c.peek(), Some(b'M' | b'm')) {
        return Err(PathError::Malformed);
    }

    loop {
        c.skip_wsp();
        let Some(cmd) = c.peek() else { break };
        c.pos += 1;
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        c.skip_wsp();
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = c.pair()?;
                let (x, y) = (ox + x, oy + y);
                out.move_to(x, y)?;
                (cx, cy) = (x, y);
                // Further pairs are implicit linetos.
                while c.more_args() {
                    let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                    let (x, y) = c.pair()?;
                    out.line_to(dx + x, dy + y)?;
                    (cx, cy) = (dx + x, dy + y);
                }
                last = LastControl::None;
            }
            b'Z' => {
                out.close();
                (cx, cy) = out.start;
                last = LastControl::None;
            }
            b'L' => loop {
                let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                let (x, y) = c.pair()?;
                out.line_to(dx + x, dy + y)?;
                (cx, cy) = (dx + x, dy + y);
                last = LastControl::None;
                if !c.more_args() {
                    break;
                }
            },
            b'H' => loop {
                let x = c.number()? as f64 + if rel { cx } else { 0.0 };
                out.line_to(x, cy)?;
                cx = x;
                last = LastControl::None;
                if !c.more_args() {
                    break;
                }
            },
            b'V' => loop {
                let y = c.number()? as f64 + if rel { cy } else { 0.0 };
                out.line_to(cx, y)?;
                cy = y;
                last = LastControl::None;
                if !c.more_args() {
                    break;
                }
            },
            b'C' => loop {
                let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                let (x1, y1) = c.pair()?;
                c.skip_comma_wsp();
                let (x2, y2) = c.pair()?;
                c.skip_comma_wsp();
                let (x, y) = c.pair()?;
                let (x1, y1, x2, y2, x, y) =
                    (dx + x1, dy + y1, dx + x2, dy + y2, dx + x, dy + y);
                out.cubic_to(x1, y1, x2, y2, x, y)?;
                (cx, cy) = (x, y);
                last = LastControl::Cubic(x2, y2);
                if !c.more_args() {
                    break;
                }
            },
            b'S' => loop {
                let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                let (x1, y1) = match last {
                    LastControl::Cubic(px, py) => (2.0 * cx - px, 2.0 * cy - py),
                    _ => (cx, cy),
                };
                let (x2, y2) = c.pair()?;
                c.skip_comma_wsp();
                let (x, y) = c.pair()?;
                let (x2, y2, x, y) = (dx + x2, dy + y2, dx + x, dy + y);
                out.cubic_to(x1, y1, x2, y2, x, y)?;
                (cx, cy) = (x, y);
                last = LastControl::Cubic(x2, y2);
                if !c.more_args() {
                    break;
                }
            },
            b'Q' => loop {
                let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                let (x1, y1) = c.pair()?;
                c.skip_comma_wsp();
                let (x, y) = c.pair()?;
                let (x1, y1, x, y) = (dx + x1, dy + y1, dx + x, dy + y);
                out.quad_to(x1, y1, x, y)?;
                (cx, cy) = (x, y);
                last = LastControl::Quad(x1, y1);
                if !c.more_args() {
                    break;
                }
            },
            b'T' => loop {
                let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                let (x1, y1) = match last {
                    LastControl::Quad(px, py) => (2.0 * cx - px, 2.0 * cy - py),
                    _ => (cx, cy),
                };
                let (x, y) = c.pair()?;
                let (x, y) = (dx + x, dy + y);
                out.quad_to(x1, y1, x, y)?;
                (cx, cy) = (x, y);
                last = LastControl::Quad(x1, y1);
                if !c.more_args() {
                    break;
                }
            },
            b'A' => loop {
                let (dx, dy) = if rel { (cx, cy) } else { (0.0, 0.0) };
                let rx = c.number()? as f64;
                c.skip_comma_wsp();
                let ry = c.number()? as f64;
                c.skip_comma_wsp();
                let rotation = c.number()? as f64;
                c.skip_comma_wsp();
                let large_arc = c.flag()?;
                c.skip_comma_wsp();
                let sweep = c.flag()?;
                c.skip_comma_wsp();
                let (x, y) = c.pair()?;
                let (x, y) = (dx + x, dy + y);
                arc_to(
                    &mut out,
                    (cx, cy),
                    rx,
                    ry,
                    rotation,
                    large_arc,
                    sweep,
                    (x, y),
                )?;
                (cx, cy) = (x, y);
                last = LastControl::None;
                if !c.more_args() {
                    break;
                }
            },
            _ => return Err(PathError::Malformed),
        }
    }

    if !out.drew {
        return Err(PathError::Malformed);
    }
    Ok(out.segments)
}

/// Append an SVG elliptical arc as cubics, per SVG 2's implementation
/// notes: endpoint to center parameterization, out-of-range radii
/// scaled up, a zero radius drawn as a line, identical endpoints
/// dropped. At most four cubics, one per quarter turn.
#[allow(clippy::too_many_arguments)]
fn arc_to(
    out: &mut PathWriter<'_>,
    (x1, y1): (f64, f64),
    rx: f64,
    ry: f64,
    rotation_deg: f64,
    large_arc: bool,
    sweep: bool,
    (x2, y2): (f64, f64),
) -> Result<(), PathError> {
    use std::f64::consts::{FRAC_PI_2, TAU};

    if x1 == x2 && y1 == y2 {
        return Ok(());
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return out.line_to(x2, y2);
    }
    let (sin_phi, cos_phi) = rotation_deg.to_radians().sin_cos();

    // Step 1: the midpoint, in the ellipse's rotated frame.
    let (hx, hy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let x1p = cos_phi * hx + sin_phi * hy;
    let y1p = -sin_phi * hx + cos_phi * hy;

    // Out-of-range radii: scale up until the endpoints fit.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
    }

    // Step 2: the center in the rotated frame. Rounding can push the
    // radicand slightly below zero when the radii were just scaled.
    let (rx2, ry2) = (rx * rx, ry * ry);
    let num = rx2 * ry2 - rx2 * y1p * y1p - ry2 * x1p * x1p;
    let den = rx2 * y1p * y1p + ry2 * x1p * x1p;
    let mut coef = if den > 0.0 {
        (num / den).max(0.0).sqrt()
    } else {
        0.0
    };
    if large_arc == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;

    // Step 3: the center, and the start and sweep angles.
    let center_x = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let center_y = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
    let angle =
        |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let (ux, uy) = ((x1p - cxp) / rx, (y1p - cyp) / ry);
    let (vx, vy) = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy) % TAU;
    if !sweep && delta > 0.0 {
        delta -= TAU;
    } else if sweep && delta < 0.0 {
        delta += TAU;
    }
    if !delta.is_finite() {
        return Err(PathError::Malformed);
    }

    let pieces = ((delta.abs() / FRAC_PI_2).ceil() as usize).clamp(1, 4);
    let step = delta / pieces as f64;
    let kappa = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (sin_t, cos_t) = t.sin_cos();
        (
            center_x + rx * cos_t * cos_phi - ry * sin_t * sin_phi,
            center_y + rx * cos_t * sin_phi + ry * sin_t * cos_phi,
        )
    };
    let tangent = |t: f64| {
        let (sin_t, cos_t) = t.sin_cos();
        (
            -rx * sin_t * cos_phi - ry * cos_t * sin_phi,
            -rx * sin_t * sin_phi + ry * cos_t * cos_phi,
        )
    };
    let mut t = theta1;
    let mut from = (x1, y1);
    for i in 0..pieces {
        let next = t + step;
        // Land exactly on the endpoint the path data gave.
        let to = if i + 1 == pieces {
            (x2, y2)
        } else {
            point(next)
        };
        let (d1x, d1y) = tangent(t);
        let (d2x, d2y) = tangent(next);
        out.cubic_to(
            from.0 + kappa * d1x,
            from.1 + kappa * d1y,
            to.0 - kappa * d2x,
            to.1 - kappa * d2y,
            to.0,
            to.1,
        )?;
        from = to;
        t = next;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VB24: ViewBox = ViewBox {
        x: 0.0,
        y: 0.0,
        w: 24.0,
        h: 24.0,
    };

    fn layer(payload: &str) -> Result<PathLayer, PathError> {
        let mut glyph = parse(payload.as_bytes(), VB24)?;
        assert_eq!(glyph.layers.len(), 1);
        Ok(glyph.layers.remove(0))
    }

    fn end(segment: Option<&Segment>) -> Option<(f32, f32)> {
        match *segment? {
            Segment::MoveTo(x, y) | Segment::LineTo(x, y) => Some((x, y)),
            Segment::QuadTo(_, _, x, y) => Some((x, y)),
            Segment::CubicTo(_, _, _, _, x, y) => Some((x, y)),
            Segment::Close => None,
        }
    }

    fn segments(data: &str) -> Vec<Segment> {
        layer(&format!("fill=nonzero,fg:{data}")).unwrap().segments
    }

    #[test]
    fn view_box() {
        assert_eq!(
            parse_view_box(b"0,-960,960,960"),
            Some(ViewBox {
                x: 0.0,
                y: -960.0,
                w: 960.0,
                h: 960.0
            })
        );
        assert_eq!(parse_view_box(b"0 0 24 24"), Some(VB24));
        assert_eq!(parse_view_box(b"0,0,0,24"), None);
        assert_eq!(parse_view_box(b"0,0,24"), None);
        assert_eq!(parse_view_box(b"0,0,24,24,1"), None);
        assert_eq!(parse_view_box(b"0,0,1e7,24"), None);
    }

    #[test]
    fn layers_and_headers() {
        let glyph = parse(
            b"fill=nonzero,fg:M0 0h4v4z|fill=evenodd,#e5534b:M1 1h2v2z|stroke=2,fg@0.2:M0 0l4 4",
            VB24,
        )
        .unwrap();
        assert_eq!(glyph.layers.len(), 3);
        assert_eq!(glyph.layers[0].paint, LayerPaint::Fill(FillRule::NonZero));
        assert_eq!(glyph.layers[0].color, LayerColor::Foreground { alpha: 255 });
        assert_eq!(glyph.layers[1].paint, LayerPaint::Fill(FillRule::EvenOdd));
        assert_eq!(
            glyph.layers[1].color,
            LayerColor::Rgba([0xe5, 0x53, 0x4b, 0xff])
        );
        assert_eq!(glyph.layers[2].paint, LayerPaint::Stroke { width: 2.0 });
        assert_eq!(glyph.layers[2].color, LayerColor::Foreground { alpha: 51 });
        assert!(!glyph.is_monochrome());
        assert!(glyph.uses_foreground());
    }

    #[test]
    fn bad_headers() {
        for payload in [
            "M0 0h4",
            "fill,fg:M0 0h4",
            "fill=winding,fg:M0 0h4",
            "fill=nonzero:M0 0h4",
            "fill=nonzero,red:M0 0h4",
            "fill=nonzero,#fff:M0 0h4",
            "fill=nonzero,fg@1.5:M0 0h4",
            "stroke=0,fg:M0 0h4",
            "stroke=25,fg:M0 0h4",
            "stroke=-1,fg:M0 0h4",
        ] {
            assert_eq!(layer(payload), Err(PathError::Malformed), "{payload}");
        }
    }

    #[test]
    fn only_printable_ascii() {
        for payload in [
            "fill=nonzero,fg:M0 0\th4",
            "fill=nonzero,fg:M0 0h4\n",
            "fill=nonzero,fg:M0 0h4\u{e9}",
        ] {
            assert_eq!(layer(payload), Err(PathError::Malformed), "{payload:?}");
        }
    }

    #[test]
    fn grammar_edges_svg_2_allows() {
        // Exponents, signs as separators, packed decimals.
        assert_eq!(
            segments("M1e1 1E1l.5.5-1+1"),
            vec![
                Segment::MoveTo(10.0, 10.0),
                Segment::LineTo(10.5, 10.5),
                Segment::LineTo(9.5, 11.5),
            ]
        );
        // Implicit repetition after M is a lineto; relative after m.
        assert_eq!(
            segments("m1 1 2 0 0 2"),
            vec![
                Segment::MoveTo(1.0, 1.0),
                Segment::LineTo(3.0, 1.0),
                Segment::LineTo(3.0, 3.0),
            ]
        );
        // Packed arc flags, and implicit repetition of arcs.
        assert!(segments("M2 12a4 4 0 0010 0").len() > 1);
        let two = segments("M2 12a4 4 0 0 0 4 0 4 4 0 0 0 4 0");
        assert_eq!(end(two.last()), Some((10.0, 12.0)));
    }

    #[test]
    fn grammar_errors_reject_the_whole_payload() {
        for data in [
            "",
            "L1 1",
            "M0 0L1",
            "M0 0L10. 10",
            "M0 0L1 1,",
            "M0 0X1 1",
            "M0 0a4 4 0 2 0 4 0",
            "M0 0,L1 1",
            "M0 0L1e 1",
            "M0 0",
        ] {
            assert_eq!(
                layer(&format!("fill=nonzero,fg:{data}")),
                Err(PathError::Malformed),
                "{data:?}"
            );
        }
    }

    #[test]
    fn shorthand_curves_reflect_the_previous_control() {
        assert_eq!(
            segments("M0 0C0 4 4 4 4 0S8 -4 8 0"),
            vec![
                Segment::MoveTo(0.0, 0.0),
                Segment::CubicTo(0.0, 4.0, 4.0, 4.0, 4.0, 0.0),
                Segment::CubicTo(4.0, -4.0, 8.0, -4.0, 8.0, 0.0),
            ]
        );
        assert_eq!(
            segments("M0 0Q2 4 4 0T8 0"),
            vec![
                Segment::MoveTo(0.0, 0.0),
                Segment::QuadTo(2.0, 4.0, 4.0, 0.0),
                Segment::QuadTo(6.0, -4.0, 8.0, 0.0),
            ]
        );
        // With no curve before it, S's first control is the current point.
        assert_eq!(
            segments("M1 1S2 2 3 3")[1],
            Segment::CubicTo(1.0, 1.0, 2.0, 2.0, 3.0, 3.0)
        );
    }

    #[test]
    fn drawing_after_close_reopens_at_the_subpath_start() {
        assert_eq!(
            segments("M2 2h4v4zl1 1"),
            vec![
                Segment::MoveTo(2.0, 2.0),
                Segment::LineTo(6.0, 2.0),
                Segment::LineTo(6.0, 6.0),
                Segment::Close,
                Segment::MoveTo(2.0, 2.0),
                Segment::LineTo(3.0, 3.0),
            ]
        );
    }

    #[test]
    fn arcs_follow_svg_edge_rules() {
        // A zero radius is a line.
        assert_eq!(segments("M0 0A0 4 0 0 0 4 0")[1], Segment::LineTo(4.0, 0.0));
        // Identical endpoints draw nothing, so the layer is empty.
        assert_eq!(
            layer("fill=nonzero,fg:M4 4A2 2 0 0 0 4 4"),
            Err(PathError::Malformed)
        );
        // Radii too small for the endpoints are scaled up: a half circle.
        let half = segments("M0 12A1 1 0 0 1 24 12");
        assert_eq!(half.len(), 3, "two quarter turns");
        // A full-ish arc never takes more than four cubics.
        let big = segments("M12 2a10 10 0 1 1 -0.01 0");
        assert!(big.len() <= 5);
        // Lucide uses radii far larger than its 24-unit box; the arc
        // stays near the chord, inside the bounds.
        assert!(segments("M2 12A1112 1112 0 0 1 22 12").len() >= 2);
    }

    #[test]
    fn points_must_stay_near_the_view_box() {
        // 32 viewBox sizes of margin: 24 * 32 = 768.
        assert!(layer("fill=nonzero,fg:M0 0L792 0").is_ok());
        assert_eq!(
            layer("fill=nonzero,fg:M0 0L793 0"),
            Err(PathError::TooLarge)
        );
        assert_eq!(
            layer("fill=nonzero,fg:M0 0C-90000 -90000 90000 90000 12 12"),
            Err(PathError::TooLarge)
        );
        assert_eq!(
            layer("fill=nonzero,fg:M0 0L1e39 0"),
            Err(PathError::TooLarge)
        );
        // A negative origin moves the box with it.
        let material = ViewBox {
            x: 0.0,
            y: -960.0,
            w: 960.0,
            h: 960.0,
        };
        assert!(parse(b"fill=nonzero,fg:M480-480h100v100z", material).is_ok());
    }

    #[test]
    fn segment_and_layer_caps() {
        let at_cap = format!(
            "fill=nonzero,fg:M0 0{}",
            "l1 0 -1 0 ".repeat(MAX_SEGMENTS / 2)
        );
        assert!(layer(&at_cap).is_ok());
        let over = format!("{at_cap}l1 0");
        assert_eq!(layer(&over), Err(PathError::TooLarge));
        // The cap is per registration, across layers.
        let half = format!(
            "fill=nonzero,fg:M0 0{}",
            "l1 0 -1 0 ".repeat(MAX_SEGMENTS / 4)
        );
        assert!(parse(format!("{half}|{half}").as_bytes(), VB24).is_ok());
        assert_eq!(
            parse(
                format!("{half}|{half}|fill=nonzero,fg:M0 0h1").as_bytes(),
                VB24
            ),
            Err(PathError::TooLarge)
        );
        let layers = vec!["fill=nonzero,fg:M0 0h1"; MAX_LAYERS + 1].join("|");
        assert_eq!(parse(layers.as_bytes(), VB24), Err(PathError::TooLarge));
    }

    #[test]
    fn real_icons() {
        // Lucide "folder", stroked.
        let folder = "stroke=2,fg:M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z";
        let glyph = parse(folder.as_bytes(), VB24).unwrap();
        assert!(glyph.is_monochrome());
        // Phosphor duotone heart, as two layers.
        let vb = parse_view_box(b"0,0,256,256").unwrap();
        let heart = "fill=nonzero,fg@0.2:M232 102c0 66-104 122-104 122S24 168 24 102A54 54 0 0 1 78 48c22.59 0 41.94 12.31 50 32 8.06-19.69 27.41-32 50-32a54 54 0 0 1 54 54Z|fill=nonzero,fg:M178 40c-20.65 0-38.73 8.88-50 23.89C116.73 48.88 98.65 40 78 40a62.07 62.07 0 0 0-62 62c0 70 103.79 126.66 108.21 129a8 8 0 0 0 7.58 0C136.21 228.66 240 172 240 102a62.07 62.07 0 0 0-62-62Z";
        let glyph = parse(heart.as_bytes(), vb).unwrap();
        assert_eq!(glyph.layers.len(), 2);
        assert!(glyph.is_monochrome());
    }
}
