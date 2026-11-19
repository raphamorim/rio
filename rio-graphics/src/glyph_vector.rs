// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! Glyph Protocol `fmt=vg` payloads: SVG's path data and paint
//! model sent as text. Not an SVG document: there is no XML.
//!
//! A payload is one or more layers separated by `|`, drawn in order.
//! Each layer is a header, `:`, then SVG 2 path data:
//!
//! ```text
//! fill=nonzero,fg:M4 4h16v16H4Z|stroke=2/butt/miter,#e5534bff:M6 12h12
//! clip=1,nonzero:M0 0h12v24H0Z|fill=nonzero,lin(0 0 24 0 / 0 fg / 1 #e5534b):M2 2h20v20H2Z
//! ```
//!
//! A layer is one of:
//!
//! - `fill=nonzero` or `fill=evenodd`, then a colour.
//! - `stroke=<width>[/<cap>[/<join>]]`, then a colour. Caps are `round`
//!   (the default), `butt` or `square`; joins are `round` (the
//!   default), `bevel` or `miter`, optionally followed by its limit
//!   (`miter4`, the default limit being 4).
//! - `clip=<n>,nonzero` or `clip=<n>,evenodd`: the path clips the next
//!   `n` layers, which can't be clips themselves (one level).
//!
//! Any header may end with `,m a b c d e f`, a matrix like SVG's
//! `matrix(a b c d e f)` from the layer's coordinates to the viewBox,
//! so a converter doesn't have to bake transforms into path data (arcs
//! under a non-uniform scale don't stay arcs). The path and any
//! gradient on the layer are in the layer's coordinates; bounds and
//! path length are checked after the matrix, and strokes are drawn
//! through it, so a non-uniform scale draws an elliptical pen as in
//! SVG. A matrix, a layer's or a gradient's, may stretch or shrink a
//! length at most [`MAX_MATRIX_STRETCH`] times.
//!
//! A colour is `fg` (the cell's foreground when the glyph is drawn),
//! `fg@<alpha 0..1>`, `#rrggbb`, `#rrggbbaa`, or a gradient:
//!
//! ```text
//! lin(x1 y1 x2 y2 [spread] [m a b c d e f] / offset colour / offset colour ...)
//! rad(fx fy fr cx cy r [spread] [m a b c d e f] / offset colour ...)
//! ```
//!
//! `lin` runs from (x1, y1) to (x2, y2); `rad` is a two-point conical
//! gradient from the focal circle (fx, fy, fr) to the circle (cx, cy,
//! r), which covers SVG's radial gradient including its focal point.
//! `spread` is `pad` (the default), `reflect` or `repeat`. The optional
//! matrix maps gradient space to the viewBox, as SVG's
//! `matrix(a b c d e f)`. A gradient has 2 to [`MAX_STOPS`] stops with
//! offsets from 0 to 1 that never decrease, and a stop's colour is
//! `fg`, `fg@<alpha>`, `#rrggbb` or `#rrggbbaa`.
//!
//! Coordinates are in the registration's viewBox (`vb=x,y,w,h`), y
//! down, as in SVG.
//!
//! Parsing happens once, at registration, and is strict so every
//! terminal draws the same thing: the payload is printable ASCII, the
//! path data follows the SVG 2 grammar (exponents, packed arc flags
//! and implicit command repetition included), and any error rejects
//! the whole payload instead of drawing up to it. Numbers are parsed
//! without regard to locale. Arcs are expanded into cubics here, so
//! the renderer only sees lines, quadratics and cubics.
//!
//! Limits keep drawing cheap, since a glyph is rasterized while a frame
//! is being built. Every point, control points included, must lie
//! within [`MAX_EXTENT_MULTIPLE`] viewBox sizes of the viewBox; a
//! registration holds at most [`MAX_SEGMENTS`] segments, counted after
//! arcs are expanded, and [`MAX_LAYERS`] layers; and its total path
//! length is capped by [`MAX_INK_MULTIPLE`]. Rasterizing cost follows
//! how much path crosses the cell, not how many segments or layers
//! carry it, so the length cap is what bounds it, gradient overdraw
//! included: together these keep the worst case near a millisecond and
//! a half to draw one cell with tiny-skia.

/// Most segments one registration may hold, counted after arcs are
/// expanded into cubics, clip paths included. Moves and closes count
/// too, so the cap also bounds the memory a registration keeps. The
/// largest of 584 icons sampled from nine sets needs 226; the largest
/// of 600 emoji sampled from Twemoji, OpenMoji, Noto and Fluent Flat
/// needs 955.
pub const MAX_SEGMENTS: usize = 2048;

/// Most layers one registration may hold, clip layers included. Emoji
/// are dozens of differently coloured shapes; the largest of the 600
/// sampled needs 83. Converters can't merge same-colour fills to save
/// layers, since overlapping shapes drawn in opposite directions cancel
/// under nonzero winding.
pub const MAX_LAYERS: usize = 128;

/// Most stops one gradient may have. Noto's emoji use at most 8.
pub const MAX_STOPS: usize = 16;

/// How far outside the viewBox a point may lie, in viewBox sizes (the
/// larger of its width and height). None of the 584 sampled icons
/// reaches a tenth of a size outside.
pub const MAX_EXTENT_MULTIPLE: f32 = 1.0;

/// Most path one registration may draw, in viewBox sizes: the length of
/// every line, plus every curve's control polygon (which bounds the
/// curve), across all layers. A stroked segment also costs one stroke
/// width, for the join or cap drawn around its end. 1,023 cubics
/// looping to the margin and back fit the other limits and draw 8,680
/// sizes of path, taking 1.4 ms to fill and 12 ms to stroke into an
/// 80x160 cell. Under this cap the worst case found takes 1.4 ms: 16
/// full-cell squares with a 16-stop radial gradient, a full-cell square
/// costing 4 sizes. The most any of the 584 sampled icons uses is 14.3,
/// and of the 600 sampled emoji 41.
pub const MAX_INK_MULTIPLE: f32 = 64.0;

/// How much a matrix (a layer's or a gradient's) may stretch or shrink
/// a length: its singular values must lie within `1/MAX_MATRIX_STRETCH`
/// and `MAX_MATRIX_STRETCH`. Invertible alone would let a layer keep
/// coordinates and stroke widths of 1e30 that a 1e-30 scale brings back
/// into the viewBox, far outside what a rasterizer is tested with. The
/// matrices in 608 converted emoji stretch from 0.29 to 59.
pub const MAX_MATRIX_STRETCH: f64 = 4096.0;

/// Most a viewBox's width may exceed its height, or the other way round.
/// The limits are measured in one side (see [`ViewBox::size_for`]), and
/// a thinner viewBox is not an icon.
pub const MAX_VIEW_BOX_ASPECT: f32 = 4096.0;

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

/// How the viewBox is scaled into the cell (§8.9.3), as SVG's
/// `preserveAspectRatio` `meet` (contain) and `slice` (cover), or
/// `none` (stretch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fit {
    #[default]
    Contain,
    Cover,
    Stretch,
}

impl ViewBox {
    /// The viewBox size the limits are measured in: the side `fit` maps
    /// onto the cell. `contain` fits the larger side, while `cover` and
    /// `stretch` scale the smaller one up to the cell, so a limit in the
    /// larger side would let a thin viewBox draw path many times the
    /// cell's size. With this, the path drawn is at most
    /// [`MAX_INK_MULTIPLE`] times the cell's larger pixel dimension in
    /// every mode.
    pub fn size_for(&self, fit: Fit) -> f32 {
        match fit {
            Fit::Contain => self.w.max(self.h),
            Fit::Cover | Fit::Stretch => self.w.min(self.h),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineCap {
    Round,
    Butt,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineJoin {
    Round,
    Bevel,
    Miter,
}

/// How a stroke is drawn. Width is in viewBox units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
    /// Used when `join` is `Miter`; at least 1.
    pub miter_limit: f32,
}

/// A solid colour: the cell's foreground, or a fixed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Solid {
    /// The cell's foreground colour, with this alpha on top of its own.
    Foreground { alpha: u8 },
    /// Straight (not premultiplied) sRGB.
    Rgba([u8; 4]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spread {
    Pad,
    Reflect,
    Repeat,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientShape {
    Linear {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
    },
    /// Two-point conical: from the focal circle to the outer circle.
    Radial {
        fx: f32,
        fy: f32,
        fr: f32,
        cx: f32,
        cy: f32,
        r: f32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub offset: f32,
    pub color: Solid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub shape: GradientShape,
    pub spread: Spread,
    /// Gradient space to viewBox, as SVG's `matrix(a b c d e f)`.
    pub transform: [f32; 6],
    pub stops: Vec<GradientStop>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LayerColor {
    Solid(Solid),
    Gradient(Box<Gradient>),
}

impl LayerColor {
    fn solids(&self) -> impl Iterator<Item = &Solid> {
        let (one, many) = match self {
            LayerColor::Solid(s) => (Some(s), &[][..]),
            LayerColor::Gradient(g) => (None, &g.stops[..]),
        };
        one.into_iter().chain(many.iter().map(|stop| &stop.color))
    }
}

/// What a layer does with its path.
#[derive(Debug, Clone, PartialEq)]
pub enum LayerOp {
    Fill {
        rule: FillRule,
        color: LayerColor,
    },
    Stroke {
        style: StrokeStyle,
        color: LayerColor,
    },
    /// Clip the next `count` layers to this path.
    Clip {
        rule: FillRule,
        count: usize,
    },
}

/// One drawing command, in its layer's coordinates (the viewBox's,
/// unless the layer has a matrix).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VectorLayer {
    pub op: LayerOp,
    /// In the layer's own coordinates; `transform` maps them to the
    /// viewBox.
    pub segments: Vec<Segment>,
    /// The layer's `m a b c d e f`, as SVG's `matrix(a b c d e f)`.
    /// A gradient on the layer is in the same coordinates as its path.
    pub transform: Option<[f32; 6]>,
}

/// A validated `fmt=vg` registration.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorGlyph {
    pub view_box: ViewBox,
    pub layers: Vec<VectorLayer>,
}

impl VectorGlyph {
    fn colors(&self) -> impl Iterator<Item = &LayerColor> {
        self.layers.iter().filter_map(|layer| match &layer.op {
            LayerOp::Fill { color, .. } | LayerOp::Stroke { color, .. } => Some(color),
            LayerOp::Clip { .. } => None,
        })
    }

    /// Every colour, gradient stops included, is the foreground, so the
    /// glyph can be kept as an alpha mask and tinted at draw time, like
    /// `glyf`.
    pub fn is_monochrome(&self) -> bool {
        self.colors()
            .flat_map(LayerColor::solids)
            .all(|solid| matches!(solid, Solid::Foreground { .. }))
    }

    /// Some colour, gradient stops included, is the foreground.
    pub fn uses_foreground(&self) -> bool {
        self.colors()
            .flat_map(LayerColor::solids)
            .any(|solid| matches!(solid, Solid::Foreground { .. }))
    }
}

/// Why a payload was rejected. The wire parser maps these onto the
/// register errors: `malformed_payload` and `outline_too_large`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorError {
    Malformed,
    TooLarge,
}

/// Parse a `vb=x,y,w,h` value. Separators are commas or spaces, as in
/// SVG's `viewBox`. Width and height must be positive, and neither more
/// than [`MAX_VIEW_BOX_ASPECT`] times the other.
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
    let in_ratio = w <= h * MAX_VIEW_BOX_ASPECT && h <= w * MAX_VIEW_BOX_ASPECT;
    (w > 0.0 && h > 0.0 && in_ratio).then_some(ViewBox { x, y, w, h })
}

/// Parse and validate a payload against its viewBox, with the limits
/// measured in the side `fit` maps onto the cell.
pub fn parse(
    payload: &[u8],
    view_box: ViewBox,
    fit: Fit,
) -> Result<VectorGlyph, VectorError> {
    if payload.is_empty() || !payload.iter().all(|b| (0x20..=0x7E).contains(b)) {
        return Err(VectorError::Malformed);
    }
    let size = view_box.size_for(fit);
    let bounds = Bounds::new(view_box, size);
    let mut budget = Budget {
        segments: MAX_SEGMENTS,
        ink: (size * MAX_INK_MULTIPLE) as f64,
    };
    let mut layers = Vec::new();
    for raw_layer in payload.split(|&b| b == b'|') {
        if layers.len() == MAX_LAYERS {
            return Err(VectorError::TooLarge);
        }
        let colon = raw_layer
            .iter()
            .position(|&b| b == b':')
            .ok_or(VectorError::Malformed)?;
        let (op, transform) = parse_header(&raw_layer[..colon], size)?;
        let stroke_width = match &op {
            LayerOp::Stroke { style, .. } => style.width as f64 * scale_of(transform),
            LayerOp::Fill { .. } | LayerOp::Clip { .. } => 0.0,
        };
        let mut segments = parse_path_data(
            &raw_layer[colon + 1..],
            &bounds,
            &mut budget,
            stroke_width,
            transform,
        )?;
        // The registry keeps this for the session.
        segments.shrink_to_fit();
        layers.push(VectorLayer {
            op,
            segments,
            transform,
        });
    }
    check_clips(&layers)?;
    layers.shrink_to_fit();
    Ok(VectorGlyph { view_box, layers })
}

/// Every clip covers layers that exist and aren't clips themselves.
fn check_clips(layers: &[VectorLayer]) -> Result<(), VectorError> {
    let mut clipped_left = 0usize;
    for (i, layer) in layers.iter().enumerate() {
        if let LayerOp::Clip { count, .. } = layer.op {
            if clipped_left > 0 || i + count >= layers.len() {
                return Err(VectorError::Malformed);
            }
            clipped_left = count;
        } else {
            clipped_left = clipped_left.saturating_sub(1);
        }
    }
    Ok(())
}

/// The least and most a matrix stretches a length by: its singular
/// values.
fn stretch(m: [f32; 6]) -> (f64, f64) {
    let [a, b, c, d] = [m[0], m[1], m[2], m[3]].map(f64::from);
    let sum = a * a + b * b + c * c + d * d;
    let det = (a * d - b * c).abs();
    let most = ((sum + (sum * sum - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt();
    let least = if most > 0.0 { det / most } else { 0.0 };
    (least, most)
}

/// The most a layer's matrix stretches a length by: a stroke width is
/// capped and charged in viewBox units with it, so no rotation or shear
/// draws a wider pen than the cap allows.
fn scale_of(transform: Option<[f32; 6]>) -> f64 {
    transform.map_or(1.0, |m| stretch(m).1)
}

/// A layer header: `<paint>,<value>[,m a b c d e f]`, where the value is
/// a colour (or a fill rule for a clip). Returns the op and its matrix.
/// `size` is the viewBox size the limits use ([`ViewBox::size_for`]).
fn parse_header(
    raw: &[u8],
    size: f32,
) -> Result<(LayerOp, Option<[f32; 6]>), VectorError> {
    let raw = trim(raw);
    let comma = raw
        .iter()
        .position(|&b| b == b',')
        .ok_or(VectorError::Malformed)?;
    let paint_raw = trim(&raw[..comma]);
    let (rest, transform) = split_matrix(trim(&raw[comma + 1..]))?;

    if let Some(rule) = paint_raw.strip_prefix(b"fill=") {
        let op = LayerOp::Fill {
            rule: parse_fill_rule(rule)?,
            color: parse_color(rest)?,
        };
        return Ok((op, transform));
    }
    if let Some(count) = paint_raw.strip_prefix(b"clip=") {
        let count = parse_count(count)?;
        if count == 0 || count >= MAX_LAYERS {
            return Err(VectorError::Malformed);
        }
        let op = LayerOp::Clip {
            rule: parse_fill_rule(rest)?,
            count,
        };
        return Ok((op, transform));
    }
    let spec = paint_raw
        .strip_prefix(b"stroke=")
        .ok_or(VectorError::Malformed)?;
    let mut parts = spec.split(|&b| b == b'/');
    let width = parse_whole_number(parts.next().unwrap_or_default())?;
    // A stroke wider than the viewBox only costs time: it covers the
    // whole span either way. Measured in viewBox units.
    let scaled = width as f64 * scale_of(transform);
    if !(width > 0.0 && scaled <= size as f64) {
        return Err(VectorError::Malformed);
    }
    let (cap, join, miter_limit) = parse_stroke_style(parts)?;
    let op = LayerOp::Stroke {
        style: StrokeStyle {
            width,
            cap,
            join,
            miter_limit,
        },
        color: parse_color(rest)?,
    };
    Ok((op, transform))
}

/// The `/`-separated styles after a stroke's width: at most one cap
/// (`butt`, `square`) and one join (`bevel`, `miter[<limit>]`), in
/// either order, so a converter can give just a join. `round` is a cap
/// and a join, and the default for both, so it fills whichever of the
/// two is left; two of the same kind is an error.
fn parse_stroke_style<'a>(
    parts: impl Iterator<Item = &'a [u8]>,
) -> Result<(LineCap, LineJoin, f32), VectorError> {
    let (mut cap, mut join) = (None, None);
    let mut rounds = 0;
    for part in parts {
        let (slot_cap, slot_join) = match part {
            b"round" => {
                rounds += 1;
                continue;
            }
            b"butt" => (Some(LineCap::Butt), None),
            b"square" => (Some(LineCap::Square), None),
            b"bevel" => (None, Some((LineJoin::Bevel, 4.0))),
            _ => {
                let limit = part.strip_prefix(b"miter").ok_or(VectorError::Malformed)?;
                let limit = if limit.is_empty() {
                    4.0
                } else {
                    parse_whole_number(limit)?
                };
                if !(1.0..=1000.0).contains(&limit) {
                    return Err(VectorError::Malformed);
                }
                (None, Some((LineJoin::Miter, limit)))
            }
        };
        if (slot_cap.is_some() && cap.is_some())
            || (slot_join.is_some() && join.is_some())
        {
            return Err(VectorError::Malformed);
        }
        cap = cap.or(slot_cap);
        join = join.or(slot_join);
    }
    let free = cap.is_none() as usize + join.is_none() as usize;
    if rounds > free {
        return Err(VectorError::Malformed);
    }
    let (join, miter_limit) = join.unwrap_or((LineJoin::Round, 4.0));
    Ok((cap.unwrap_or(LineCap::Round), join, miter_limit))
}

/// Split `<value>[,m a b c d e f]`. A gradient's parentheses may hold
/// commas of their own, so its value runs to the closing parenthesis.
fn split_matrix(rest: &[u8]) -> Result<(&[u8], Option<[f32; 6]>), VectorError> {
    let end = if rest.starts_with(b"lin(") || rest.starts_with(b"rad(") {
        rest.iter()
            .position(|&b| b == b')')
            .ok_or(VectorError::Malformed)?
            + 1
    } else {
        rest.iter().position(|&b| b == b',').unwrap_or(rest.len())
    };
    let (value, tail) = rest.split_at(end);
    let tail = trim(tail);
    if tail.is_empty() {
        return Ok((trim(value), None));
    }
    let matrix = tail
        .strip_prefix(b",")
        .map(trim)
        .and_then(|t| t.strip_prefix(b"m"))
        .ok_or(VectorError::Malformed)?;
    let mut c = Cursor { s: matrix, pos: 0 };
    let transform = read_matrix(&mut c)?;
    c.skip_wsp();
    if !c.eof() {
        return Err(VectorError::Malformed);
    }
    Ok((trim(value), Some(transform)))
}

/// Six numbers after an `m`: an affine matrix that stretches no length
/// by more than [`MAX_MATRIX_STRETCH`] or less than its inverse, so it
/// is invertible too.
fn read_matrix(c: &mut Cursor<'_>) -> Result<[f32; 6], VectorError> {
    c.skip_wsp();
    let mut m = [0f32; 6];
    for (i, value) in m.iter_mut().enumerate() {
        if i > 0 {
            c.skip_comma_wsp();
        }
        *value = c.number()?;
    }
    let (least, most) = stretch(m);
    if !(least >= 1.0 / MAX_MATRIX_STRETCH && most <= MAX_MATRIX_STRETCH) {
        return Err(VectorError::Malformed);
    }
    Ok(m)
}

fn parse_fill_rule(raw: &[u8]) -> Result<FillRule, VectorError> {
    match raw {
        b"nonzero" => Ok(FillRule::NonZero),
        b"evenodd" => Ok(FillRule::EvenOdd),
        _ => Err(VectorError::Malformed),
    }
}

/// A count in plain decimal digits.
fn parse_count(raw: &[u8]) -> Result<usize, VectorError> {
    if raw.is_empty() || raw.len() > 4 || !raw.iter().all(u8::is_ascii_digit) {
        return Err(VectorError::Malformed);
    }
    Ok(raw.iter().fold(0, |n, d| n * 10 + (d - b'0') as usize))
}

fn parse_color(raw: &[u8]) -> Result<LayerColor, VectorError> {
    if let Some(body) = raw.strip_prefix(b"lin(") {
        return parse_gradient(body, false);
    }
    if let Some(body) = raw.strip_prefix(b"rad(") {
        return parse_gradient(body, true);
    }
    parse_solid(raw).map(LayerColor::Solid)
}

fn parse_solid(raw: &[u8]) -> Result<Solid, VectorError> {
    if raw == b"fg" {
        return Ok(Solid::Foreground { alpha: 0xFF });
    }
    if let Some(alpha_raw) = raw.strip_prefix(b"fg@") {
        let alpha = parse_whole_number(alpha_raw)?;
        if !(0.0..=1.0).contains(&alpha) {
            return Err(VectorError::Malformed);
        }
        return Ok(Solid::Foreground {
            alpha: (alpha * 255.0).round() as u8,
        });
    }
    let hex = raw.strip_prefix(b"#").ok_or(VectorError::Malformed)?;
    parse_hex_color(hex)
        .map(Solid::Rgba)
        .ok_or(VectorError::Malformed)
}

/// The inside of `lin(...)` or `rad(...)`, closing parenthesis included.
fn parse_gradient(body: &[u8], radial: bool) -> Result<LayerColor, VectorError> {
    let body = body.strip_suffix(b")").ok_or(VectorError::Malformed)?;
    let mut parts = body.split(|&b| b == b'/');
    let mut c = Cursor {
        s: parts.next().unwrap_or_default(),
        pos: 0,
    };
    c.skip_wsp();
    let mut geometry = [0f32; 6];
    let n = if radial { 6 } else { 4 };
    for (i, value) in geometry.iter_mut().take(n).enumerate() {
        if i > 0 {
            c.skip_comma_wsp();
        }
        *value = c.number()?;
    }
    let shape = if radial {
        let [fx, fy, fr, cx, cy, r] = geometry;
        if fr < 0.0 || r < 0.0 {
            return Err(VectorError::Malformed);
        }
        GradientShape::Radial {
            fx,
            fy,
            fr,
            cx,
            cy,
            r,
        }
    } else {
        let [x1, y1, x2, y2, ..] = geometry;
        GradientShape::Linear { x1, y1, x2, y2 }
    };

    c.skip_wsp();
    let mut spread = Spread::Pad;
    for (word, value) in [
        (&b"pad"[..], Spread::Pad),
        (b"reflect", Spread::Reflect),
        (b"repeat", Spread::Repeat),
    ] {
        let rest = &c.s[c.pos..];
        // A whole word: `padm 1 0 0 1 0 0` is not a spread and a matrix.
        if rest.starts_with(word) && matches!(rest.get(word.len()), None | Some(b' ')) {
            c.pos += word.len();
            spread = value;
            break;
        }
    }
    c.skip_wsp();
    let mut transform = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    if c.peek() == Some(b'm') {
        c.pos += 1;
        transform = read_matrix(&mut c)?;
    }
    c.skip_wsp();
    if !c.eof() {
        return Err(VectorError::Malformed);
    }

    let mut stops = Vec::new();
    let mut last = 0.0f32;
    for raw_stop in parts {
        if stops.len() == MAX_STOPS {
            return Err(VectorError::TooLarge);
        }
        let raw_stop = trim(raw_stop);
        let mut c = Cursor {
            s: raw_stop,
            pos: 0,
        };
        let offset = c.number()?;
        if !(last..=1.0).contains(&offset) {
            return Err(VectorError::Malformed);
        }
        last = offset;
        if c.peek() != Some(b' ') {
            return Err(VectorError::Malformed);
        }
        let color = parse_solid(trim(&raw_stop[c.pos..]))?;
        stops.push(GradientStop { offset, color });
    }
    if stops.len() < 2 {
        return Err(VectorError::Malformed);
    }
    Ok(LayerColor::Gradient(Box::new(Gradient {
        shape,
        spread,
        transform,
        stops,
    })))
}

fn parse_hex_color(hex: &[u8]) -> Option<[u8; 4]> {
    // `from_str_radix` would also take a sign: `#+f+f+f`.
    if (hex.len() != 6 && hex.len() != 8) || !hex.iter().all(u8::is_ascii_hexdigit) {
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
fn parse_whole_number(raw: &[u8]) -> Result<f32, VectorError> {
    let mut cursor = Cursor { s: raw, pos: 0 };
    let value = cursor.number()?;
    if cursor.eof() {
        Ok(value)
    } else {
        Err(VectorError::Malformed)
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
    /// `size` is the viewBox size the limits use ([`ViewBox::size_for`]).
    fn new(vb: ViewBox, size: f32) -> Self {
        let margin = size * MAX_EXTENT_MULTIPLE;
        Self {
            x0: vb.x - margin,
            y0: vb.y - margin,
            x1: vb.x + vb.w + margin,
            y1: vb.y + vb.h + margin,
        }
    }

    fn check(&self, x: f64, y: f64) -> Result<(f32, f32), VectorError> {
        let (x, y) = (x as f32, y as f32);
        if x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1 {
            Ok((x, y))
        } else {
            // NaN fails the comparisons and lands here too.
            Err(VectorError::TooLarge)
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
    fn number(&mut self) -> Result<f32, VectorError> {
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
            return Err(VectorError::Malformed);
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
            .map_err(|_| VectorError::Malformed)?;
        let value: f64 = text.parse().map_err(|_| VectorError::Malformed)?;
        let value = value as f32;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(VectorError::TooLarge)
        }
    }

    fn flag(&mut self) -> Result<bool, VectorError> {
        let flag = match self.peek() {
            Some(b'0') => false,
            Some(b'1') => true,
            _ => return Err(VectorError::Malformed),
        };
        self.pos += 1;
        Ok(flag)
    }

    fn pair(&mut self) -> Result<(f64, f64), VectorError> {
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

/// What a registration may still spend, shared by its layers.
struct Budget {
    segments: usize,
    /// Path length left, in viewBox units; see [`MAX_INK_MULTIPLE`].
    ink: f64,
}

/// A point in the layer's coordinates and where it lands in the viewBox.
type Placed = ((f32, f32), (f64, f64));

struct PathWriter<'b> {
    segments: Vec<Segment>,
    bounds: &'b Bounds,
    budget: &'b mut Budget,
    /// The layer's stroke width, or 0 for a fill: each segment of a
    /// stroke also costs one width of ink, for the join or cap drawn
    /// around its end.
    stroke_width: f64,
    /// The layer's matrix, applied before bounds and length are checked.
    transform: Option<[f64; 6]>,
    /// Where the last segment ended, in viewBox units.
    pen: (f64, f64),
    /// A `Z` closed the subpath and no `M` has opened the next one yet.
    needs_move: bool,
    /// The subpath's start, in the layer's coordinates and in viewBox
    /// units.
    start: (f64, f64),
    start_vb: (f64, f64),
}

fn distance((ax, ay): (f64, f64), (bx, by): (f64, f64)) -> f64 {
    (bx - ax).hypot(by - ay)
}

impl PathWriter<'_> {
    /// Every segment is charged, moves and closes included: otherwise
    /// `M0 0Z` repeated would fill 64 KiB with segments the cap never
    /// sees.
    fn spend(&mut self) -> Result<(), VectorError> {
        self.budget.segments = self
            .budget
            .segments
            .checked_sub(1)
            .ok_or(VectorError::TooLarge)?;
        Ok(())
    }

    /// Charge a segment that draws something.
    fn spend_drawing(&mut self) -> Result<(), VectorError> {
        self.spend()
    }

    /// Charge `length` of path (a line, or a curve's control polygon,
    /// which bounds the curve's length) plus a stroke's width.
    fn spend_ink(&mut self, length: f64) -> Result<(), VectorError> {
        self.budget.ink -= length + self.stroke_width;
        if self.budget.ink < 0.0 {
            return Err(VectorError::TooLarge);
        }
        Ok(())
    }

    /// A point as stored (layer coordinates, f32) and as checked
    /// (viewBox units): bounds and path length apply after the matrix.
    fn place(&self, x: f64, y: f64) -> Result<Placed, VectorError> {
        let (vx, vy) = match self.transform {
            None => (x, y),
            Some([a, b, c, d, e, f]) => (a * x + c * y + e, b * x + d * y + f),
        };
        let (cx, cy) = self.bounds.check(vx, vy)?;
        Ok(((x as f32, y as f32), (cx as f64, cy as f64)))
    }

    fn move_to(&mut self, x: f64, y: f64) -> Result<(), VectorError> {
        self.spend()?;
        let ((x, y), vb) = self.place(x, y)?;
        // No length, but a stroked subpath gets caps.
        self.spend_ink(0.0)?;
        self.segments.push(Segment::MoveTo(x, y));
        self.start = (x as f64, y as f64);
        self.start_vb = vb;
        self.pen = vb;
        self.needs_move = false;
        Ok(())
    }

    /// A drawing command right after `Z` starts a new subpath at the
    /// closed one's start point, as SVG specifies.
    fn ensure_open(&mut self) -> Result<(), VectorError> {
        if self.needs_move {
            let (x, y) = self.start;
            self.move_to(x, y)?;
        }
        Ok(())
    }

    fn line_to(&mut self, x: f64, y: f64) -> Result<(), VectorError> {
        self.ensure_open()?;
        self.spend_drawing()?;
        let ((x, y), to) = self.place(x, y)?;
        self.spend_ink(distance(self.pen, to))?;
        self.pen = to;
        self.segments.push(Segment::LineTo(x, y));
        Ok(())
    }

    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> Result<(), VectorError> {
        self.ensure_open()?;
        self.spend_drawing()?;
        let ((cx, cy), c) = self.place(cx, cy)?;
        let ((x, y), to) = self.place(x, y)?;
        self.spend_ink(distance(self.pen, c) + distance(c, to))?;
        self.pen = to;
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
    ) -> Result<(), VectorError> {
        self.ensure_open()?;
        self.spend_drawing()?;
        let ((c1x, c1y), c1) = self.place(c1x, c1y)?;
        let ((c2x, c2y), c2) = self.place(c2x, c2y)?;
        let ((x, y), to) = self.place(x, y)?;
        self.spend_ink(distance(self.pen, c1) + distance(c1, c2) + distance(c2, to))?;
        self.pen = to;
        self.segments
            .push(Segment::CubicTo(c1x, c1y, c2x, c2y, x, y));
        Ok(())
    }

    fn close(&mut self) -> Result<(), VectorError> {
        if !self.needs_move {
            self.spend()?;
            self.spend_ink(distance(self.pen, self.start_vb))?;
            self.pen = self.start_vb;
            self.segments.push(Segment::Close);
            self.needs_move = true;
        }
        Ok(())
    }
}

fn parse_path_data(
    raw: &[u8],
    bounds: &Bounds,
    budget: &mut Budget,
    stroke_width: f64,
    transform: Option<[f32; 6]>,
) -> Result<Vec<Segment>, VectorError> {
    let mut c = Cursor { s: raw, pos: 0 };
    let mut out = PathWriter {
        segments: Vec::new(),
        bounds,
        budget,
        stroke_width,
        transform: transform.map(|m| m.map(f64::from)),
        pen: (0.0, 0.0),
        needs_move: false,
        start: (0.0, 0.0),
        start_vb: (0.0, 0.0),
    };
    let (mut cx, mut cy) = (0.0f64, 0.0f64);
    let mut last = LastControl::None;

    c.skip_wsp();
    // Path data starts with a moveto.
    if !matches!(c.peek(), Some(b'M' | b'm')) {
        return Err(VectorError::Malformed);
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
                out.close()?;
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
            _ => return Err(VectorError::Malformed),
        }
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
) -> Result<(), VectorError> {
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
        return Err(VectorError::Malformed);
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

    fn layer(payload: &str) -> Result<VectorLayer, VectorError> {
        let mut glyph = parse(payload.as_bytes(), VB24, Fit::Contain)?;
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
            Fit::Contain,
        )
        .unwrap();
        assert_eq!(glyph.layers.len(), 3);
        assert_eq!(
            glyph.layers[0].op,
            LayerOp::Fill {
                rule: FillRule::NonZero,
                color: LayerColor::Solid(Solid::Foreground { alpha: 255 })
            }
        );
        assert_eq!(
            glyph.layers[1].op,
            LayerOp::Fill {
                rule: FillRule::EvenOdd,
                color: LayerColor::Solid(Solid::Rgba([0xe5, 0x53, 0x4b, 0xff]))
            }
        );
        assert_eq!(
            glyph.layers[2].op,
            LayerOp::Stroke {
                style: StrokeStyle {
                    width: 2.0,
                    cap: LineCap::Round,
                    join: LineJoin::Round,
                    miter_limit: 4.0
                },
                color: LayerColor::Solid(Solid::Foreground { alpha: 51 })
            }
        );
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
            "fill=nonzero,#+f+f+f:M0 0h4",
            "fill=nonzero,#ff00zz:M0 0h4",
            "fill=nonzero,fg@1.5:M0 0h4",
            "stroke=0,fg:M0 0h4",
            "stroke=25,fg:M0 0h4",
            "stroke=-1,fg:M0 0h4",
            // Stroke styles.
            "stroke=2/flat,fg:M0 0h4",
            "stroke=2/butt/sharp,fg:M0 0h4",
            "stroke=2/butt/miter0.5,fg:M0 0h4",
            "stroke=2/butt/miter/x,fg:M0 0h4",
            "stroke=2/,fg:M0 0h4",
            // Clip headers.
            "clip=0,nonzero:M0 0h4",
            "clip=x,nonzero:M0 0h4",
            "clip=1,fg:M0 0h4",
        ] {
            assert_eq!(layer(payload), Err(VectorError::Malformed), "{payload}");
        }
    }

    #[test]
    fn only_printable_ascii() {
        for payload in [
            "fill=nonzero,fg:M0 0\th4",
            "fill=nonzero,fg:M0 0h4\n",
            "fill=nonzero,fg:M0 0h4\u{e9}",
        ] {
            assert_eq!(layer(payload), Err(VectorError::Malformed), "{payload:?}");
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
        ] {
            assert_eq!(
                layer(&format!("fill=nonzero,fg:{data}")),
                Err(VectorError::Malformed),
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
        // Identical endpoints draw nothing: only the moveto is left.
        assert_eq!(
            segments("M4 4A2 2 0 0 0 4 4"),
            vec![Segment::MoveTo(4.0, 4.0)]
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
        // One viewBox size of margin on every side: [-24, 48].
        assert!(layer("fill=nonzero,fg:M0 0L48 0").is_ok());
        assert!(layer("fill=nonzero,fg:M0 0L0 -24").is_ok());
        assert_eq!(
            layer("fill=nonzero,fg:M0 0L49 0"),
            Err(VectorError::TooLarge)
        );
        assert_eq!(
            layer("fill=nonzero,fg:M0 0L0 -25"),
            Err(VectorError::TooLarge)
        );
        // Control points count, not just the points the curve passes.
        assert_eq!(
            layer("fill=nonzero,fg:M0 0C-30 0 54 24 12 12"),
            Err(VectorError::TooLarge)
        );
        assert_eq!(
            layer("fill=nonzero,fg:M0 0Q12 60 24 0"),
            Err(VectorError::TooLarge)
        );
        // So do the cubics an arc becomes: a 30-unit radius bulges past
        // the margin even though both endpoints are inside the box.
        assert_eq!(
            layer("fill=nonzero,fg:M2 12A30 30 0 1 1 22 12"),
            Err(VectorError::TooLarge)
        );
        assert_eq!(
            layer("fill=nonzero,fg:M0 0L1e39 0"),
            Err(VectorError::TooLarge)
        );
        // A negative origin moves the box with it.
        let material = ViewBox {
            x: 0.0,
            y: -960.0,
            w: 960.0,
            h: 960.0,
        };
        assert!(
            parse(b"fill=nonzero,fg:M480-480h100v100z", material, Fit::Contain).is_ok()
        );
    }

    fn op(payload: &str) -> LayerOp {
        layer(payload).unwrap().op
    }

    fn stroke_style(payload: &str) -> StrokeStyle {
        match op(payload) {
            LayerOp::Stroke { style, .. } => style,
            other => panic!("not a stroke: {other:?}"),
        }
    }

    fn gradient(payload: &str) -> Gradient {
        match op(payload) {
            LayerOp::Fill {
                color: LayerColor::Gradient(g),
                ..
            } => *g,
            other => panic!("not a gradient fill: {other:?}"),
        }
    }

    #[test]
    fn stroke_caps_and_joins() {
        let s = stroke_style("stroke=2,fg:M0 0h4");
        assert_eq!((s.cap, s.join), (LineCap::Round, LineJoin::Round));
        let s = stroke_style("stroke=2/butt,fg:M0 0h4");
        assert_eq!((s.cap, s.join), (LineCap::Butt, LineJoin::Round));
        let s = stroke_style("stroke=1.5/square/bevel,fg:M0 0h4");
        assert_eq!(
            (s.width, s.cap, s.join),
            (1.5, LineCap::Square, LineJoin::Bevel)
        );
        let s = stroke_style("stroke=2/butt/miter,fg:M0 0h4");
        assert_eq!((s.join, s.miter_limit), (LineJoin::Miter, 4.0));
        let s = stroke_style("stroke=2/round/miter10,fg:M0 0h4");
        assert_eq!((s.join, s.miter_limit), (LineJoin::Miter, 10.0));
    }

    #[test]
    fn linear_and_radial_gradients() {
        let g = gradient("fill=nonzero,lin(0 0 24 0 / 0 #ff0000 / 1 fg@0.5):M0 0h4v4z");
        assert_eq!(
            g.shape,
            GradientShape::Linear {
                x1: 0.0,
                y1: 0.0,
                x2: 24.0,
                y2: 0.0
            }
        );
        assert_eq!(g.spread, Spread::Pad);
        assert_eq!(g.transform, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(
            g.stops,
            vec![
                GradientStop {
                    offset: 0.0,
                    color: Solid::Rgba([0xff, 0, 0, 0xff])
                },
                GradientStop {
                    offset: 1.0,
                    color: Solid::Foreground { alpha: 128 }
                },
            ]
        );
        // Focal point, spread, a matrix with packed signs, three stops.
        let g = gradient(
            "fill=evenodd,rad(10 10 0 12 12 8 reflect m 1 0 0-1 0 24 / 0 fg / 0.5 #00ff0080 / 1 #0000ff):M0 0h4v4z",
        );
        assert_eq!(
            g.shape,
            GradientShape::Radial {
                fx: 10.0,
                fy: 10.0,
                fr: 0.0,
                cx: 12.0,
                cy: 12.0,
                r: 8.0
            }
        );
        assert_eq!(g.spread, Spread::Reflect);
        assert_eq!(g.transform, [1.0, 0.0, 0.0, -1.0, 0.0, 24.0]);
        assert_eq!(g.stops.len(), 3);
        // Strokes take gradients too.
        assert!(matches!(
            op("stroke=2/butt,lin(0 0 1 1 repeat / 0 fg / 1 fg@0):M0 0h4"),
            LayerOp::Stroke {
                color: LayerColor::Gradient(_),
                ..
            }
        ));
    }

    #[test]
    fn gradient_errors_reject_the_payload() {
        for colour in [
            "lin(0 0 24 0 / 0 fg)",                         // one stop
            "lin(0 0 24 / 0 fg / 1 fg)",                    // three coordinates
            "lin(0 0 24 0 / 0 fg / 1 fg",                   // unclosed
            "lin(0 0 24 0 / 0.6 fg / 0.4 fg)",              // offsets go back
            "lin(0 0 24 0 / 0 fg / 1.5 fg)",                // offset past 1
            "lin(0 0 24 0 / 0 fg / 1 red)",                 // named colour
            "lin(0 0 24 0 / 0 fg / 1 lin(0 0 1 1 / 0 fg))", // gradient stop
            "lin(0 0 24 0 / 0fg / 1 fg)",                   // no space
            "lin(0 0 24 0 sideways / 0 fg / 1 fg)",         // unknown spread
            "lin(0 0 24 0 m 1 0 0 1 0 / 0 fg / 1 fg)",      // short matrix
            "lin(0 0 24 0 m 1 2 2 4 0 0 / 0 fg / 1 fg)",    // not invertible
            "rad(0 0 -1 0 0 4 / 0 fg / 1 fg)",              // negative radius
            "rad(0 0 0 0 0 4 0 / 0 fg / 1 fg)",             // extra coordinate
            "con(0 0 24 0 / 0 fg / 1 fg)",                  // unknown kind
        ] {
            assert_eq!(
                layer(&format!("fill=nonzero,{colour}:M0 0h4v4z")),
                Err(VectorError::Malformed),
                "{colour}"
            );
        }
        let stops = |n: usize| {
            (0..n)
                .map(|i| format!(" / {} fg", i as f32 / (n - 1) as f32))
                .collect::<String>()
        };
        let at_cap = format!("fill=nonzero,lin(0 0 24 0{}):M0 0h4v4z", stops(MAX_STOPS));
        assert!(layer(&at_cap).is_ok());
        let over = format!(
            "fill=nonzero,lin(0 0 24 0{}):M0 0h4v4z",
            stops(MAX_STOPS + 1)
        );
        assert_eq!(layer(&over), Err(VectorError::TooLarge));
    }

    #[test]
    fn clip_layers_clip_the_next_layers() {
        let glyph = parse(
            b"clip=2,evenodd:M0 0h12v24H0Z|fill=nonzero,fg:M0 0h24v24H0Z|stroke=2,#ff0000:M0 12h24|fill=nonzero,fg:M0 0h1",
            VB24,
            Fit::Contain,
        )
        .unwrap();
        assert_eq!(
            glyph.layers[0].op,
            LayerOp::Clip {
                rule: FillRule::EvenOdd,
                count: 2
            }
        );
        // A clip must have that many layers after it, none of them a
        // clip: one level only.
        for payload in [
            "clip=1,nonzero:M0 0h4v4z",
            "clip=3,nonzero:M0 0h4v4z|fill=nonzero,fg:M0 0h1|fill=nonzero,fg:M0 0h1",
            "clip=2,nonzero:M0 0h4v4z|clip=1,nonzero:M0 0h1v1z|fill=nonzero,fg:M0 0h1",
        ] {
            assert_eq!(
                parse(payload.as_bytes(), VB24, Fit::Contain),
                Err(VectorError::Malformed),
                "{payload}"
            );
        }
        // After a clip's range ends, another clip may start.
        assert!(parse(
            b"clip=1,nonzero:M0 0h4v4z|fill=nonzero,fg:M0 0h1|clip=1,nonzero:M0 0h4v4z|fill=nonzero,fg:M0 0h1",
            VB24,
            Fit::Contain
        )
        .is_ok());
        // Clip paths count against the segment and length caps like a fill.
        let long = format!(
            "clip=1,nonzero:M0 0{}|fill=nonzero,fg:M0 0h1",
            "h24h-24".repeat(33)
        );
        assert_eq!(layer_count_err(&long), Err(VectorError::TooLarge));
    }

    fn layer_count_err(payload: &str) -> Result<usize, VectorError> {
        parse(payload.as_bytes(), VB24, Fit::Contain).map(|g| g.layers.len())
    }

    #[test]
    fn layer_matrices_map_the_path_into_the_view_box() {
        let l = layer("fill=nonzero,fg,m 1 0 0 1 12 0:M0 0h4v4z").unwrap();
        assert_eq!(l.transform, Some([1.0, 0.0, 0.0, 1.0, 12.0, 0.0]));
        // The path stays in its own coordinates.
        assert_eq!(l.segments[0], Segment::MoveTo(0.0, 0.0));
        // Bounds apply after the matrix: in the box once moved...
        assert!(layer("fill=nonzero,fg,m 1 0 0 1 -60 0:M60 0h4v4z").is_ok());
        // ...and out of it once moved past the margin.
        assert_eq!(
            layer("fill=nonzero,fg,m 1 0 0 1 40 0:M0 0h10v4z"),
            Err(VectorError::TooLarge)
        );
        // So does the path length: 40 short lines become 40 long ones.
        let lines = "h10h-10".repeat(20);
        assert!(layer(&format!("fill=nonzero,fg:M0 0{lines}")).is_ok());
        assert_eq!(
            layer(&format!("fill=nonzero,fg,m 4 0 0 4 0 0:M0 0{lines}")),
            Err(VectorError::TooLarge)
        );
        // A stroke width is capped in viewBox units, by the matrix's
        // largest stretch: a shear stretches more than either column
        // (1.618 here, where the longer column is 1.414).
        assert!(layer("stroke=12,fg,m 2 0 0 2 0 0:M0 0h1").is_ok());
        assert!(layer("stroke=14,fg,m 1 0 1 1 0 0:M0 0h1").is_ok());
        assert_eq!(
            layer("stroke=16,fg,m 1 0 1 1 0 0:M0 0h1"),
            Err(VectorError::Malformed)
        );
        assert_eq!(
            layer("stroke=13,fg,m 2 0 0 2 0 0:M0 0h1"),
            Err(VectorError::Malformed)
        );
        // Gradients may hold commas; the matrix follows the parenthesis.
        let g =
            layer("fill=nonzero,lin(0,0,24,0 / 0 fg / 1 fg@0),m 2 0 0 2 0 0:M0 0h4v4z")
                .unwrap();
        assert_eq!(g.transform, Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));
        assert!(matches!(
            g.op,
            LayerOp::Fill {
                color: LayerColor::Gradient(_),
                ..
            }
        ));
        // Clips take one too.
        let glyph = parse(
            b"clip=1,nonzero,m 1 0 0 1 2 2:M0 0h4v4z|fill=nonzero,fg:M0 0h1",
            VB24,
            Fit::Contain,
        )
        .unwrap();
        assert_eq!(
            glyph.layers[0].transform,
            Some([1.0, 0.0, 0.0, 1.0, 2.0, 2.0])
        );
        for header in [
            "fill=nonzero,fg,m 1 0 0 0 0 0",   // not invertible
            "fill=nonzero,fg,m 1 0 0 1 0",     // five numbers
            "fill=nonzero,fg,m 1 0 0 1 0 0 x", // trailing text
            "fill=nonzero,fg,1 0 0 1 0 0",     // no m
            "fill=nonzero,fg,",                // empty
        ] {
            assert_eq!(
                layer(&format!("{header}:M0 0h4v4z")),
                Err(VectorError::Malformed),
                "{header}"
            );
        }
    }

    #[test]
    fn foreground_gradients_keep_the_glyph_monochrome() {
        let mono = parse(
            b"fill=nonzero,lin(0 0 24 0 / 0 fg / 1 fg@0):M0 0h4v4z|clip=1,nonzero:M0 0h1v1z|fill=nonzero,fg@0.5:M0 0h1",
            VB24,
            Fit::Contain,
        )
        .unwrap();
        assert!(mono.is_monochrome());
        assert!(mono.uses_foreground());
        let mixed = parse(
            b"fill=nonzero,lin(0 0 24 0 / 0 fg / 1 #ff0000):M0 0h4v4z",
            VB24,
            Fit::Contain,
        )
        .unwrap();
        assert!(!mixed.is_monochrome());
        assert!(mixed.uses_foreground());
        let fixed = parse(
            b"fill=nonzero,rad(0 0 0 12 12 12 / 0 #000000 / 1 #ffffff):M0 0h4v4z",
            VB24,
            Fit::Contain,
        )
        .unwrap();
        assert!(!fixed.is_monochrome());
        assert!(!fixed.uses_foreground());
    }

    #[test]
    fn segment_and_layer_caps() {
        // `n` short linetos that go back and forth, staying in the box
        // and well under the path length cap.
        let back_and_forth =
            |n: usize| "h.5h-.5".repeat(n / 2) + if n % 2 == 1 { "h.5" } else { "" };
        // The moveto is a segment too.
        let at_cap = format!("fill=nonzero,fg:M0 0{}", back_and_forth(MAX_SEGMENTS - 1));
        assert!(layer(&at_cap).is_ok());
        let over = format!("{at_cap}h-.5");
        assert_eq!(layer(&over), Err(VectorError::TooLarge));
        // The cap is per registration, across layers.
        let half = format!(
            "fill=nonzero,fg:M0 0{}",
            back_and_forth(MAX_SEGMENTS / 2 - 1)
        );
        assert!(parse(format!("{half}|{half}").as_bytes(), VB24, Fit::Contain).is_ok());
        assert_eq!(
            parse(
                format!("{half}|{half}|fill=nonzero,fg:M0 0h1").as_bytes(),
                VB24,
                Fit::Contain
            ),
            Err(VectorError::TooLarge)
        );
        let layers = vec!["fill=nonzero,fg:M0 0h1"; MAX_LAYERS + 1].join("|");
        assert_eq!(
            parse(layers.as_bytes(), VB24, Fit::Contain),
            Err(VectorError::TooLarge)
        );
    }

    #[test]
    fn moves_and_closes_count_against_the_cap() {
        // Subpaths that draw nothing still cost memory for the whole
        // session: each `M x yZ` is two segments.
        let pairs = (MAX_SEGMENTS - 2) / 2;
        let fits = format!("fill=nonzero,fg:M0 0h1{}", "M1 1Z".repeat(pairs));
        assert!(layer(&fits).is_ok());
        let flood = format!("fill=nonzero,fg:M0 0h1{}", "M1 1Z".repeat(pairs + 1));
        assert_eq!(layer(&flood), Err(VectorError::TooLarge));
        // A run of bare movetos is no cheaper.
        let moves = format!("fill=nonzero,fg:M0 0h1{}", "M1 1".repeat(MAX_SEGMENTS));
        assert_eq!(layer(&moves), Err(VectorError::TooLarge));
        // The implicit moveto after a close is charged as well: three
        // segments per `h.1z` after the first, short enough that the
        // path length cap isn't what stops it.
        let reopen = format!("fill=nonzero,fg:M0 0{}", "h.1z".repeat(MAX_SEGMENTS / 2));
        assert_eq!(layer(&reopen), Err(VectorError::TooLarge));
    }

    #[test]
    fn total_path_length_is_capped() {
        // Each cubic loops to the margin and back: its control polygon
        // is about 204 units, and 64 viewBox sizes allow 1,536.
        let loops = |n: usize| {
            let mut s = String::from("fill=nonzero,fg:M12 12");
            for i in 0..n {
                s.push_str(if i % 2 == 0 {
                    "C-24 -24 48 48 12 12"
                } else {
                    "C48 48 -24 -24 12 12"
                });
            }
            s
        };
        // Nine segments, far under the segment cap and inside the
        // bounds: only the length is over.
        assert!(layer(&loops(7)).is_ok());
        assert_eq!(layer(&loops(8)), Err(VectorError::TooLarge));

        // A stroke costs its width per segment as well: 64 short
        // segments are nothing filled, too much at the widest stroke.
        let short = "h.1h-.1".repeat(32);
        assert!(layer(&format!("fill=nonzero,fg:M12 12{short}")).is_ok());
        assert!(layer(&format!("stroke=2,fg:M12 12{short}")).is_ok());
        assert_eq!(
            layer(&format!("stroke=24,fg:M12 12{short}")),
            Err(VectorError::TooLarge)
        );

        // Closing counts the segment back to the start.
        let square = "M0 0h24v24h-24z"; // 96 units
        let squares = |n: usize| format!("fill=nonzero,fg:{}", square.repeat(n));
        assert!(layer(&squares(16)).is_ok());
        assert_eq!(layer(&squares(17)), Err(VectorError::TooLarge));

        // The budget is per registration, across layers.
        let half = format!("fill=nonzero,fg:{}", square.repeat(8));
        assert!(parse(format!("{half}|{half}").as_bytes(), VB24, Fit::Contain).is_ok());
        assert_eq!(
            parse(
                format!("{half}|{half}|fill=nonzero,fg:M0 0h1").as_bytes(),
                VB24,
                Fit::Contain
            ),
            Err(VectorError::TooLarge)
        );

        // The cap scales with the viewBox.
        let vb = parse_view_box(b"0,0,960,960").unwrap();
        let big = format!("fill=nonzero,fg:{}", "M0 0h960v960h-960z".repeat(16));
        assert!(parse(big.as_bytes(), vb, Fit::Contain).is_ok());
    }

    /// Endpoints of every segment, `None` for a close.
    fn ends(data: &str) -> Vec<Option<(f32, f32)>> {
        segments(data).iter().map(|s| end(Some(s))).collect()
    }

    #[test]
    fn conformance_implicit_commands_and_subpaths() {
        let p = |x: f32, y: f32| Some((x, y));
        // Pairs after a moveto are linetos, absolute after M.
        assert_eq!(ends("M2 2 8 2 8 8"), vec![p(2., 2.), p(8., 2.), p(8., 8.)]);
        // A second relative moveto starts from the current point, and
        // its own extra pairs are relative linetos.
        assert_eq!(
            ends("m2 2 4 0m0 4 4 0"),
            vec![p(2., 2.), p(6., 2.), p(6., 6.), p(10., 6.)]
        );
        // H and V repeat their single coordinate.
        assert_eq!(
            ends("M1 1H5 9V4 7"),
            vec![p(1., 1.), p(5., 1.), p(9., 1.), p(9., 4.), p(9., 7.)]
        );
        assert_eq!(
            ends("M1 1h2 2v3 3"),
            vec![p(1., 1.), p(3., 1.), p(5., 1.), p(5., 4.), p(5., 7.)]
        );
        // After Z the current point is the subpath start, so a relative
        // moveto counts from there.
        assert_eq!(
            ends("M4 4h6v6zm2 2h1"),
            vec![
                p(4., 4.),
                p(10., 4.),
                p(10., 10.),
                None,
                p(6., 6.),
                p(7., 6.)
            ]
        );
        // Consecutive movetos are kept; the last one starts the drawing.
        assert_eq!(ends("M1 1M3 3L5 5"), vec![p(1., 1.), p(3., 3.), p(5., 5.)]);
        // Repeated closes collapse into one.
        assert_eq!(
            ends("M0 0h4v4zzz"),
            vec![p(0., 0.), p(4., 0.), p(4., 4.), None]
        );
        // Signs separate numbers; so do commas, once, between arguments.
        assert_eq!(
            ends("M0,0L4-4+4-4 8,8,2 2"),
            vec![p(0., 0.), p(4., -4.), p(4., -4.), p(8., 8.), p(2., 2.)]
        );
    }

    #[test]
    fn conformance_shorthand_reflection_rules() {
        // S reflects only a cubic's control: after Q its first control
        // is the current point.
        assert_eq!(
            segments("M0 0Q4 8 8 0S12 8 16 0")[2],
            Segment::CubicTo(8., 0., 12., 8., 16., 0.)
        );
        // T reflects only a quadratic's control: after C it is the
        // current point.
        assert_eq!(
            segments("M0 0C0 8 8 8 8 0T16 0")[2],
            Segment::QuadTo(8., 0., 16., 0.)
        );
        // Chained T keeps reflecting the control it inferred.
        assert_eq!(
            segments("M0 0T4 4T8 0")[1..],
            [
                Segment::QuadTo(0., 0., 4., 4.),
                Segment::QuadTo(8., 8., 8., 0.)
            ]
        );
        // Relative t after relative q: the reflection is absolute.
        assert_eq!(
            segments("M0 6q2 6 4 0t4 0")[1..],
            [
                Segment::QuadTo(2., 12., 4., 6.),
                Segment::QuadTo(6., 0., 8., 6.)
            ]
        );
        // A moveto in between forgets the previous control.
        assert_eq!(
            segments("M0 0C0 8 8 8 8 0M10 0S14 8 16 0")[3],
            Segment::CubicTo(10., 0., 14., 8., 16., 0.)
        );
    }

    #[test]
    fn conformance_arc_flags() {
        // Flags are one character, so they pack against each other and
        // against the next coordinate in every combination.
        let reference = segments("M4 12a8 8 0 0 1 16 0");
        for packed in [
            "M4 12a8 8 0 0116 0",
            "M4 12a8,8,0,0,1,16,0",
            "M4 12a8 8 0 01 16 0",
        ] {
            assert_eq!(segments(packed), reference, "{packed}");
        }
        // A sign ends the flag as well.
        assert_eq!(
            end(segments("M20 12a8 8 0 0 1-16 0").last()),
            Some((4., 12.))
        );
        // Sweep picks the side: positive-angle (1) goes over the top in
        // y-down coordinates, so the midpoint control rises above y=12.
        let over = segments("M4 12a8 8 0 0 1 16 0");
        let under = segments("M4 12a8 8 0 0 0 16 0");
        let mid_y = |s: &[Segment]| match s[1] {
            Segment::CubicTo(.., y) => y,
            _ => unreachable!(),
        };
        assert!(mid_y(&over) < 12.0 && mid_y(&under) > 12.0);
        // Anything other than a lone 0 or 1 rejects the payload, as does
        // a number that swallowed the flags (ry = 401, then no flags).
        for data in [
            "M4 12a8 8 0 2 1 16 0",
            "M4 12a8 8 0 0 7 16 0",
            "M4 12a8 8 0 -1 1 16 0",
            "M4 12a8 8 0 1 -0 16 0",
            "M4 12a8 401 18 0",
        ] {
            assert_eq!(
                layer(&format!("fill=nonzero,fg:{data}")),
                Err(VectorError::Malformed),
                "{data}"
            );
        }
    }

    #[test]
    fn paths_that_draw_nothing_are_valid() {
        // SVG 2 allows path data that only moves or closes; it draws
        // nothing. Emoji sets ship such shapes (Twemoji has a lone
        // `M14 12z`), and rejecting them would drop the whole emoji.
        for data in ["M0 0", "M1 1M3 3", "m0 0z", "M14 12zm0 0z"] {
            let l = layer(&format!("fill=nonzero,fg:{data}")).expect(data);
            assert!(l
                .segments
                .iter()
                .all(|s| matches!(s, Segment::MoveTo(..) | Segment::Close)));
        }
        // They still count against the caps.
        let many = format!("fill=nonzero,fg:{}", "M1 1".repeat(MAX_SEGMENTS + 1));
        assert_eq!(layer(&many), Err(VectorError::TooLarge));
    }

    #[test]
    fn conformance_malformed_data_rejects_everything() {
        // SVG renders up to the error; here nothing registers, so every
        // terminal agrees on the result.
        for data in [
            "M0 0L4 4 8",
            "M0 0L4#4",
            "M0 0L4,,4",
            "M0 0L,4 4",
            "M0 0L4 4,",
            "M0 0 L4 4 Z 1 1",
            "M0 0L.",
            "M0 0L1e+ 4",
            "  ",
        ] {
            assert_eq!(
                layer(&format!("fill=nonzero,fg:{data}")),
                Err(VectorError::Malformed),
                "{data:?}"
            );
        }
    }

    #[test]
    fn numbers_split_where_svg_parsers_split_them() {
        let p = |x: f32, y: f32| Some((x, y));
        // A second `.` starts a new number, as does a sign after an
        // exponent's digits.
        assert_eq!(ends("M.5.5L1e1.5"), vec![p(0.5, 0.5), p(10.0, 0.5)]);
        assert_eq!(ends("M1e-1-1"), vec![p(0.1, -1.0)]);
        assert_eq!(ends("M+.5-.5L1E1 -0"), vec![p(0.5, -0.5), p(10.0, -0.0)]);
        // Underflow is zero; overflow past f32 is too large, not wrapped.
        assert_eq!(ends("M1e-50 0"), vec![p(0.0, 0.0)]);
        assert_eq!(
            layer("fill=nonzero,fg:M0 0L1e39 0"),
            Err(VectorError::TooLarge)
        );
        // A trailing dot is not part of a number. svgtypes (usvg) takes
        // `1.` as 1; Chrome and Firefox reject it, and so do we, since
        // the strict reading is the one every terminal can agree on.
        assert_eq!(layer("fill=nonzero,fg:M1. 2"), Err(VectorError::Malformed));
    }

    #[test]
    fn tiny_arc_radii_scale_up_instead_of_flattening() {
        // SVG 2 draws a straight line only for a radius of exactly zero;
        // any other radius too small for the endpoints is scaled up, so
        // this is a half circle bulging 5 units (kurbo would flatten
        // radii under 1e-5 into a line).
        let half = segments("M2 12a1e-6 1e-6 0 0 1 10 0");
        assert_eq!(half.len(), 3, "two quarter-turn cubics");
        let Segment::CubicTo(_, _, _, _, x, y) = half[1] else {
            panic!("{half:?}")
        };
        assert!((x - 7.0).abs() < 1e-3 && (y - 7.0).abs() < 1e-3, "{x} {y}");
    }

    #[test]
    fn arcs_land_on_their_endpoint_and_honour_rotation() {
        // Relative arcs end exactly where the path data says, so the
        // next relative command starts from the right place.
        assert_eq!(
            ends("M1 1a3 2 30 0 1 7.3 4.1l1 0")
                .last()
                .copied()
                .flatten(),
            Some((9.3, 5.1))
        );
        // An ellipse rotated a quarter turn is the same as one with its
        // radii swapped: same cubics, to within rounding.
        let rotated = segments("M4 12A8 4 90 0 1 20 12");
        let swapped = segments("M4 12A4 8 0 0 1 20 12");
        assert_eq!(rotated.len(), swapped.len());
        for (a, b) in rotated.iter().zip(&swapped) {
            let (
                Segment::CubicTo(a1, a2, a3, a4, a5, a6),
                Segment::CubicTo(b1, b2, b3, b4, b5, b6),
            ) = (a, b)
            else {
                assert_eq!(a, b);
                continue;
            };
            for (u, v) in [(a1, b1), (a2, b2), (a3, b3), (a4, b4), (a5, b5), (a6, b6)] {
                assert!((u - v).abs() < 1e-3, "{rotated:?} vs {swapped:?}");
            }
        }
    }

    #[test]
    fn arcs_count_their_cubics_against_the_segment_cap() {
        // A near-full circle is four cubics; with the moveto that's five
        // segments, so it doesn't fit when only four are left.
        let filler =
            |n: usize| "h.5h-.5".repeat(n / 2) + if n % 2 == 1 { "h.5" } else { "" };
        let arc = "M12 2a10 10 0 1 1 -0.01 0";
        let room = MAX_SEGMENTS - 5;
        let fits = format!(
            "fill=nonzero,fg:M0 0{}|fill=nonzero,fg:{arc}",
            filler(room - 1)
        );
        assert!(parse(fits.as_bytes(), VB24, Fit::Contain).is_ok());
        let over = format!("fill=nonzero,fg:M0 0{}|fill=nonzero,fg:{arc}", filler(room));
        assert_eq!(
            parse(over.as_bytes(), VB24, Fit::Contain),
            Err(VectorError::TooLarge)
        );
    }

    #[test]
    fn exactly_the_layer_cap_is_accepted() {
        let at_cap = vec!["fill=nonzero,fg:M0 0h1"; MAX_LAYERS].join("|");
        assert_eq!(
            parse(at_cap.as_bytes(), VB24, Fit::Contain)
                .unwrap()
                .layers
                .len(),
            MAX_LAYERS
        );
        // Empty layers aren't layers: `|` must separate two of them.
        for payload in [
            "fill=nonzero,fg:M0 0h1|",
            "|fill=nonzero,fg:M0 0h1",
            "fill=nonzero,fg:M0 0h1||fill=nonzero,fg:M0 0h1",
        ] {
            assert_eq!(
                parse(payload.as_bytes(), VB24, Fit::Contain),
                Err(VectorError::Malformed),
                "{payload}"
            );
        }
    }

    #[test]
    fn matrices_have_a_bounded_stretch() {
        let with = |m: &str| layer(&format!("fill=nonzero,fg,m {m}:M0 0h1"));
        let grad = |m: &str| {
            layer(&format!(
                "fill=nonzero,lin(0 0 1 0 m {m} / 0 fg / 1 #000000):M0 0h1"
            ))
        };
        for m in [
            "4096 0 0 4096 0 0",
            "0.000244140625 0 0 0.000244140625 0 0", // 1/4096
            "0 1 -1 0 0 0",                          // a quarter turn
            "1 0 0 -1 0 24",                         // a flip
            "0.29 0 0 59 0 0",                       // the emoji extremes
        ] {
            assert!(grad(m).is_ok(), "gradient {m}");
            if m != "4096 0 0 4096 0 0" {
                assert!(with(m).is_ok(), "layer {m}");
            }
        }
        for m in [
            "4097 0 0 1 0 0",
            "0.0002 0 0 1 0 0",
            "1e-30 0 0 1e-30 0 0", // invertible, but coordinates of 1e30
            "1 0 0 0 0 0",         // singular
            "1 1 1 1 0 0",         // singular, every entry non-zero
            "3000 3000 0 1 0 0",   // a shear past the stretch cap
        ] {
            assert_eq!(with(m), Err(VectorError::Malformed), "layer {m}");
            assert_eq!(grad(m), Err(VectorError::Malformed), "gradient {m}");
        }
    }

    #[test]
    fn matrix_syntax() {
        let ok =
            |header: &str| layer(&format!("{header}:M0 0h4v4z")).map(|l| l.transform);
        let id = Some([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        // Spaces and commas between numbers, like path data.
        assert_eq!(ok("fill=nonzero,fg,m1 0 0 1 0 0"), Ok(id));
        assert_eq!(ok("fill=nonzero,fg,m 1,0,0,1,0,0"), Ok(id));
        assert_eq!(ok("fill=nonzero,fg , m 1 0 0 1 0 0 "), Ok(id));
        assert_eq!(ok("stroke=1/butt/miter2,#112233,m 1 0 0 1 0 0"), Ok(id));
        for header in [
            "fill=nonzero,fg m 1 0 0 1 0 0",
            "fill=nonzero,fg,m 1 0 0 1 0 0,m 1 0 0 1 0 0",
            "fill=nonzero,fg,M 1 0 0 1 0 0",
            "fill=nonzero,lin(0 0 1 0 / 0 fg / 1 fg)m 1 0 0 1 0 0",
            "fill=nonzero,lin(0 0 1 0 / 0 fg / 1 fg,m 1 0 0 1 0 0",
            "fill=nonzero,fg,m 1 0 0 1 0 0)",
        ] {
            assert_eq!(ok(header), Err(VectorError::Malformed), "{header}");
        }
    }

    #[test]
    fn a_spread_is_a_whole_word() {
        let g = |inside: &str| {
            layer(&format!(
                "fill=nonzero,lin({inside} / 0 fg / 1 #000000):M0 0h1"
            ))
        };
        assert!(g("0 0 1 0 reflect").is_ok());
        assert!(g("0 0 1 0 repeat m 1 0 0 1 0 0").is_ok());
        for inside in [
            "0 0 1 0 padm 1 0 0 1 0 0",
            "0 0 1 0 reflected",
            "0 0 1 0 repeat2",
        ] {
            assert_eq!(g(inside), Err(VectorError::Malformed), "{inside}");
        }
    }

    #[test]
    fn relative_commands_under_a_matrix_stay_in_layer_space() {
        // Stored in the layer's coordinates: the renderer applies the
        // matrix, so relative steps add up before it, as in SVG.
        let l = layer("fill=nonzero,fg,m 2 0 0 2 0 0:M1 1l2 0v3z").unwrap();
        assert_eq!(
            l.segments,
            vec![
                Segment::MoveTo(1.0, 1.0),
                Segment::LineTo(3.0, 1.0),
                Segment::LineTo(3.0, 4.0),
                Segment::Close
            ]
        );
        // After Z, drawing restarts at the start in layer coordinates
        // too: (1, 1) doubled is in the box, (1, 1) + 30 doubled isn't.
        assert!(layer("fill=nonzero,fg,m 2 0 0 2 0 0:M1 1h2zh10").is_ok());
        assert_eq!(
            layer("fill=nonzero,fg,m 2 0 0 2 0 0:M1 1h2zh30"),
            Err(VectorError::TooLarge)
        );
    }

    #[test]
    fn colour_queries_skip_clip_layers() {
        let glyph = |p: &str| parse(p.as_bytes(), VB24, Fit::Contain).unwrap();
        // A clip has no colour, so it doesn't make a glyph coloured.
        let g = glyph("clip=1,nonzero:M0 0h12v24H0Z|fill=nonzero,fg@0.5:M0 0h4v4z");
        assert!(g.is_monochrome() && g.uses_foreground());
        let g = glyph("clip=1,nonzero:M0 0h12v24H0Z|fill=nonzero,#ff000080:M0 0h4v4z");
        assert!(!g.is_monochrome() && !g.uses_foreground());
        // A gradient from a fixed colour to the foreground is both.
        let g = glyph("fill=nonzero,rad(4 4 0 4 4 4 / 0 #000000 / 1 fg@0):M0 0h4v4z");
        assert!(!g.is_monochrome() && g.uses_foreground());
    }

    /// Mutations of valid payloads must never panic, and whatever they
    /// parse to must respect every limit: points (through the layer's
    /// matrix) in bounds, segments and layers under their caps.
    #[test]
    fn mutated_payloads_never_panic_or_escape_the_limits() {
        const SEEDS: &[&str] = &[
            "fill=nonzero,fg:M4 4h16v16H4Z|stroke=2/butt/miter,#e5534bff:M6 12h12",
            "clip=1,nonzero:M0 0h12v24H0Z|fill=nonzero,lin(0 0 24 0 reflect / 0 fg / 1 #e5534b):M2 2h20v20H2Z",
            "fill=evenodd,rad(9 9 0 12 12 10 m 1 0 0 1 0 0 / 0 fg@.5 / .5 #fff000 / 1 #e5534b80):m2 2c4-2 8 2 12 0s4 4 6 6q2 2 4 0t2 2a3 2 30 1 0 4 4z",
            "stroke=1.5,fg,m 0.5 0.2 -0.2 0.5 6 6:M0 0L20 0 20 20 0 20zM5 5h10v10h-10z",
        ];
        const ALPHABET: &[u8] =
            b"0123456789.-+eE ,MmLlHhVvCcSsQqTtAaZz|:=/()#@fglinradmpbuttsqrcl";
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = |n: usize| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % n as u64) as usize
        };
        let bounds = Bounds::new(VB24, 24.0);
        let mut accepted = 0;
        for _ in 0..50_000 {
            let mut bytes = SEEDS[next(SEEDS.len())].as_bytes().to_vec();
            for _ in 0..1 + next(2) {
                let at = next(bytes.len() + 1);
                // Half the edits are digits, which mostly keep the
                // grammar valid and move points, radii and matrices.
                let byte = if next(2) == 0 {
                    b'0' + next(10) as u8
                } else {
                    ALPHABET[next(ALPHABET.len())]
                };
                match next(3) {
                    0 if at < bytes.len() => bytes[at] = byte,
                    1 if at < bytes.len() => {
                        bytes.remove(at);
                    }
                    _ => bytes.insert(at, byte),
                }
            }
            let Ok(glyph) = parse(&bytes, VB24, Fit::Contain) else {
                continue;
            };
            accepted += 1;
            assert!(glyph.layers.len() <= MAX_LAYERS);
            let total: usize = glyph.layers.iter().map(|l| l.segments.len()).sum();
            assert!(total <= MAX_SEGMENTS);
            for layer in &glyph.layers {
                let m = layer.transform.unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                let place = |x: f32, y: f32| {
                    let (x, y) = (x as f64, y as f64);
                    let [a, b, c, d, e, f] = m.map(f64::from);
                    bounds
                        .check(a * x + c * y + e, b * x + d * y + f)
                        .unwrap_or_else(|_| {
                            panic!("{}", String::from_utf8_lossy(&bytes))
                        });
                };
                for segment in &layer.segments {
                    match *segment {
                        Segment::MoveTo(x, y) | Segment::LineTo(x, y) => place(x, y),
                        Segment::QuadTo(a, b, x, y) => {
                            place(a, b);
                            place(x, y);
                        }
                        Segment::CubicTo(a, b, c, d, x, y) => {
                            place(a, b);
                            place(c, d);
                            place(x, y);
                        }
                        Segment::Close => {}
                    }
                }
            }
        }
        // Enough survive mutation to exercise the checks above.
        assert!(accepted > 5_000, "only {accepted} parsed");
    }

    #[test]
    fn repeated_closes_are_free() {
        // A `Z` right after another closes nothing, so it isn't a
        // segment (§8.9.5): thousands of them still fit.
        let closes = format!("fill=nonzero,fg:M0 0h4v4{}", "z".repeat(MAX_SEGMENTS * 2));
        assert_eq!(layer(&closes).unwrap().segments.len(), 4);
    }

    #[test]
    fn path_length_is_measured_after_the_layer_matrix() {
        // 96 units per square: 16 fit in 64 sizes of 24, 17 don't.
        // Halved by a matrix each is 48, so 32 fit and 33 don't.
        let square = "M0 0h24v24h-24z";
        let at_half =
            |n: usize| format!("fill=nonzero,fg,m .5 0 0 .5 0 0:{}", square.repeat(n));
        assert!(layer(&at_half(32)).is_ok());
        assert_eq!(layer(&at_half(33)), Err(VectorError::TooLarge));
        // A stroke's per-segment width is charged through the matrix
        // too: width 12 doubled costs as much as width 24 unscaled.
        let short = "h.05h-.05".repeat(32);
        assert_eq!(
            layer(&format!("stroke=24,fg:M6 6{short}")),
            Err(VectorError::TooLarge)
        );
        assert_eq!(
            layer(&format!("stroke=12,fg,m 2 0 0 2 0 0:M6 6{short}")),
            Err(VectorError::TooLarge)
        );
        assert!(layer(&format!("stroke=12,fg:M6 6{short}")).is_ok());
    }

    #[test]
    fn numbers_too_big_for_f32_are_too_large_everywhere() {
        // A number that doesn't fit is a limit (§8.9.5), wherever it
        // appears; a number that's only out of range is malformed.
        for payload in [
            "fill=nonzero,fg:M0 0L1e39 0",
            "fill=nonzero,lin(0 0 1e39 0 / 0 fg / 1 fg):M0 0h1",
            "fill=nonzero,lin(0 0 1 0 / 0 fg / 1e39 fg):M0 0h1",
            "fill=nonzero,fg,m 1e39 0 0 1 0 0:M0 0h1",
            "stroke=1e39,fg:M0 0h1",
            "fill=nonzero,fg@1e39:M0 0h1",
        ] {
            assert_eq!(layer(payload), Err(VectorError::TooLarge), "{payload}");
        }
        for payload in [
            "fill=nonzero,lin(0 0 1 0 / 0 fg / 2 fg):M0 0h1",
            "fill=nonzero,fg,m 1e30 0 0 1 0 0:M0 0h1",
            "stroke=1e30,fg:M0 0h1",
            "fill=nonzero,fg@2:M0 0h1",
        ] {
            assert_eq!(layer(payload), Err(VectorError::Malformed), "{payload}");
        }
    }

    #[test]
    fn cover_and_stretch_measure_limits_in_the_smaller_side() {
        // `cover` and `stretch` scale the smaller side up to the cell,
        // so a thin viewBox measured by its larger side would let path
        // many times the cell's size through (kitty#10872: 7 s for
        // 1.3 KB before kitty added defences).
        let thin = parse_view_box(b"0,0,1000,1").unwrap();
        let line = b"fill=nonzero,fg:M0 0h1000";
        assert!(parse(line, thin, Fit::Contain).is_ok());
        for fit in [Fit::Cover, Fit::Stretch] {
            assert_eq!(
                parse(line, thin, fit),
                Err(VectorError::TooLarge),
                "{fit:?}"
            );
            // 64 of the smaller side still fit.
            assert!(parse(b"fill=nonzero,fg:M0 0h32h-32", thin, fit).is_ok());
            // Points stay within one smaller side of the viewBox.
            assert_eq!(
                parse(b"fill=nonzero,fg:M0 -2h1", thin, fit),
                Err(VectorError::TooLarge)
            );
            // A stroke is capped by the smaller side too.
            assert!(parse(b"stroke=1,fg:M0 0h1", thin, fit).is_ok());
            assert_eq!(
                parse(b"stroke=2,fg:M0 0h1", thin, fit),
                Err(VectorError::Malformed)
            );
        }
        // A square viewBox is the same in every mode.
        for fit in [Fit::Contain, Fit::Cover, Fit::Stretch] {
            assert_eq!(VB24.size_for(fit), 24.0);
        }
    }

    #[test]
    fn view_box_aspect_is_bounded() {
        assert!(parse_view_box(b"0,0,4096,1").is_some());
        assert!(parse_view_box(b"0,0,1,4096").is_some());
        assert!(parse_view_box(b"0,0,4097,1").is_none());
        assert!(parse_view_box(b"0 0 1 0.000001").is_none());
    }

    #[test]
    fn stroke_styles_come_in_either_order() {
        let style = |header: &str| match layer(&format!("{header},fg:M0 0h4")) {
            Ok(VectorLayer {
                op: LayerOp::Stroke { style, .. },
                ..
            }) => Ok((style.cap, style.join, style.miter_limit)),
            Ok(other) => panic!("{other:?}"),
            Err(e) => Err(e),
        };
        // A join alone, as a converter might write it.
        assert_eq!(
            style("stroke=1/miter"),
            Ok((LineCap::Round, LineJoin::Miter, 4.0))
        );
        assert_eq!(
            style("stroke=1/miter10"),
            Ok((LineCap::Round, LineJoin::Miter, 10.0))
        );
        assert_eq!(
            style("stroke=1/bevel"),
            Ok((LineCap::Round, LineJoin::Bevel, 4.0))
        );
        // Either order.
        assert_eq!(
            style("stroke=1/butt/bevel"),
            Ok((LineCap::Butt, LineJoin::Bevel, 4.0))
        );
        assert_eq!(
            style("stroke=1/bevel/butt"),
            Ok((LineCap::Butt, LineJoin::Bevel, 4.0))
        );
        // `round` is both and fills whichever is left.
        assert_eq!(
            style("stroke=1/round/butt"),
            Ok((LineCap::Butt, LineJoin::Round, 4.0))
        );
        assert_eq!(
            style("stroke=1/miter/round"),
            Ok((LineCap::Round, LineJoin::Miter, 4.0))
        );
        assert_eq!(
            style("stroke=1/round/round"),
            Ok((LineCap::Round, LineJoin::Round, 4.0))
        );
        for header in [
            "stroke=1/butt/square",
            "stroke=1/miter/bevel",
            "stroke=1/round/round/round",
            "stroke=1/butt/round/bevel/round",
            "stroke=1/round/butt/square",
            "stroke=1/miter0.5",
        ] {
            assert_eq!(style(header), Err(VectorError::Malformed), "{header}");
        }
    }

    #[test]
    fn real_icons() {
        // Lucide "folder", stroked.
        let folder = "stroke=2,fg:M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z";
        let glyph = parse(folder.as_bytes(), VB24, Fit::Contain).unwrap();
        assert!(glyph.is_monochrome());
        // Phosphor duotone heart, as two layers.
        let vb = parse_view_box(b"0,0,256,256").unwrap();
        let heart = "fill=nonzero,fg@0.2:M232 102c0 66-104 122-104 122S24 168 24 102A54 54 0 0 1 78 48c22.59 0 41.94 12.31 50 32 8.06-19.69 27.41-32 50-32a54 54 0 0 1 54 54Z|fill=nonzero,fg:M178 40c-20.65 0-38.73 8.88-50 23.89C116.73 48.88 98.65 40 78 40a62.07 62.07 0 0 0-62 62c0 70 103.79 126.66 108.21 129a8 8 0 0 0 7.58 0C136.21 228.66 240 172 240 102a62.07 62.07 0 0 0-62-62Z";
        let glyph = parse(heart.as_bytes(), vb, Fit::Contain).unwrap();
        assert_eq!(glyph.layers.len(), 2);
        assert!(glyph.is_monochrome());
    }
}
