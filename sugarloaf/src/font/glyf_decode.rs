// OpenType `glyf` simple-glyph decoder.
//
// Parses a bare TrueType simple-glyph record (as sent over the Glyph
// Protocol wire) into a neutral [`Outline`], then walks the outline to
// produce a sequence of [`PathCmd`]s suitable for feeding to any vector
// path library (zeno, lyon, skia, …).
//
// The low-level byte parsing is delegated to `read-fonts`, which backs
// `skrifa`. That keeps us on the same battle-tested parser skrifa itself
// uses for rendering real fonts. The contour walker — the part that
// turns on-curve/off-curve sequences into move/line/quad commands
// following the TrueType quadratic-Bézier rules — is local because
// skrifa's equivalent (`contour_to_path`) is `pub(crate)`.
//
// References:
//   - OpenType glyf: https://learn.microsoft.com/en-us/typography/opentype/spec/glyf
//   - Apple TrueType Reference Manual Chapter 6

use read_fonts::tables::glyf::{CurvePoint, SimpleGlyph};
use read_fonts::{FontData, FontRead};

/// Reasons a `glyf` record can be rejected. Maps onto Glyph Protocol v1
/// `reason=` error codes where applicable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// numberOfContours < 0 (composite glyph reference). Our subset does
    /// not support composites.
    Composite,
    /// instructionLength > 0. Hinting bytecode is not accepted.
    Hinted,
    /// Payload ended before the decoder expected, or a structural
    /// invariant was violated.
    Malformed,
}

/// A single decoded point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
    pub on_curve: bool,
}

/// Fully-decoded simple glyph: a list of closed contours, each a
/// `Vec<Point>`, plus the glyph's bounding box in its authoring
/// coordinate space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outline {
    pub contours: Vec<Vec<Point>>,
    pub x_min: i32,
    pub y_min: i32,
    pub x_max: i32,
    pub y_max: i32,
}

/// Decode a simple-glyph record.
pub fn decode(data: &[u8]) -> Result<Outline, DecodeError> {
    // Peek at numberOfContours before handing to read-fonts, because
    // `SimpleGlyph::read` will happily parse a composite record as a
    // simple one and give back garbage. The first two bytes of every
    // glyph record are the signed 16-bit numberOfContours.
    if data.len() < 10 {
        return Err(DecodeError::Malformed);
    }
    let num_contours = i16::from_be_bytes([data[0], data[1]]);
    if num_contours < 0 {
        return Err(DecodeError::Composite);
    }

    let glyph =
        SimpleGlyph::read(FontData::new(data)).map_err(|_| DecodeError::Malformed)?;

    if glyph.instruction_length() != 0 {
        return Err(DecodeError::Hinted);
    }

    let x_min = glyph.x_min() as i32;
    let y_min = glyph.y_min() as i32;
    let x_max = glyph.x_max() as i32;
    let y_max = glyph.y_max() as i32;

    let end_pts: Vec<usize> = glyph
        .end_pts_of_contours()
        .iter()
        .map(|v| v.get() as usize)
        .collect();
    if end_pts.is_empty() {
        return Ok(Outline {
            contours: Vec::new(),
            x_min,
            y_min,
            x_max,
            y_max,
        });
    }
    // End-points must be strictly increasing.
    for w in end_pts.windows(2) {
        if w[1] <= w[0] {
            return Err(DecodeError::Malformed);
        }
    }
    let num_points = end_pts[end_pts.len() - 1] + 1;

    let pts: Vec<CurvePoint> = glyph.points().collect();
    if pts.len() != num_points {
        return Err(DecodeError::Malformed);
    }

    // Split the flat point list into contours.
    let mut contours = Vec::with_capacity(end_pts.len());
    let mut start = 0usize;
    for &end in &end_pts {
        let mut contour = Vec::with_capacity(end - start + 1);
        for p in &pts[start..=end] {
            contour.push(Point {
                x: p.x as i32,
                y: p.y as i32,
                on_curve: p.on_curve,
            });
        }
        contours.push(contour);
        start = end + 1;
    }

    Ok(Outline {
        contours,
        x_min,
        y_min,
        x_max,
        y_max,
    })
}

/// A single path command produced by [`Outline::walk`]. Intentionally
/// does not depend on any external path library so the walker can be
/// tested in isolation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    MoveTo { x: f32, y: f32 },
    LineTo { x: f32, y: f32 },
    QuadTo { cx: f32, cy: f32, x: f32, y: f32 },
    Close,
}

impl Outline {
    /// Walk each contour and emit path commands following the TrueType
    /// quadratic-Bézier rules. `upm` is the authoring coordinate space;
    /// `pixel_size` is the target render size. Coordinates are scaled
    /// and Y-flipped so the rasterizer (which expects Y-down) sees the
    /// glyph the right way up.
    pub fn walk(&self, upm: u16, pixel_size: f32) -> Vec<PathCmd> {
        if self.contours.is_empty() || upm == 0 {
            return Vec::new();
        }
        let scale = pixel_size / upm as f32;
        let fx = |x: i32| x as f32 * scale;
        // glyf Y is up, most rasterizers want Y-down. Subtract from
        // y_max * scale so the glyph sits at (0,0) of the destination.
        let y_base = self.y_max as f32 * scale;
        let fy = |y: i32| y_base - y as f32 * scale;

        let mut out = Vec::new();
        for contour in &self.contours {
            walk_contour(contour, &fx, &fy, &mut out);
        }
        out
    }
}

fn walk_contour(
    pts: &[Point],
    fx: &impl Fn(i32) -> f32,
    fy: &impl Fn(i32) -> f32,
    out: &mut Vec<PathCmd>,
) {
    if pts.is_empty() {
        return;
    }
    let n = pts.len();

    // TrueType allows a contour to start with an off-curve point. When
    // that happens, the starting on-curve point is synthesised: if the
    // last point is on-curve, use it; otherwise use the midpoint
    // between the first and last off-curve points.
    //
    // `steps` is the number of points we'll iterate after emitting
    // MoveTo: one fewer than `n` when the start is a real point
    // (otherwise we'd revisit it via the wrap), the full `n` when the
    // start is a synthesised midpoint.
    let (start_x, start_y, start_idx, steps) = if pts[0].on_curve {
        (fx(pts[0].x), fy(pts[0].y), 1usize, n - 1)
    } else if pts[n - 1].on_curve {
        (fx(pts[n - 1].x), fy(pts[n - 1].y), 0usize, n - 1)
    } else {
        // Linear scale + flip is an affine transform, so the rendered
        // midpoint equals the midpoint of the rendered endpoints.
        let mx = (fx(pts[0].x) + fx(pts[n - 1].x)) / 2.0;
        let my = (fy(pts[0].y) + fy(pts[n - 1].y)) / 2.0;
        (mx, my, 0usize, n)
    };

    out.push(PathCmd::MoveTo {
        x: start_x,
        y: start_y,
    });

    let mut i = start_idx;
    let mut pending_off: Option<(f32, f32)> = None;
    let mut visited = 0;
    while visited < steps {
        let p = pts[i];
        let px = fx(p.x);
        let py = fy(p.y);
        if p.on_curve {
            match pending_off.take() {
                Some((cx, cy)) => out.push(PathCmd::QuadTo {
                    cx,
                    cy,
                    x: px,
                    y: py,
                }),
                None => out.push(PathCmd::LineTo { x: px, y: py }),
            }
        } else {
            match pending_off.take() {
                Some((cx, cy)) => {
                    // Two off-curves in a row: emit a quad to their
                    // implied on-curve midpoint, then carry the new
                    // off-curve as the next control.
                    let mx = (cx + px) / 2.0;
                    let my = (cy + py) / 2.0;
                    out.push(PathCmd::QuadTo {
                        cx,
                        cy,
                        x: mx,
                        y: my,
                    });
                    pending_off = Some((px, py));
                }
                None => {
                    pending_off = Some((px, py));
                }
            }
        }
        i = (i + 1) % n;
        visited += 1;
    }

    // If a control point is still pending at the end of the contour,
    // close the final curve back to the start with a quad.
    if let Some((cx, cy)) = pending_off.take() {
        out.push(PathCmd::QuadTo {
            cx,
            cy,
            x: start_x,
            y: start_y,
        });
    }

    out.push(PathCmd::Close);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Flag bits, duplicated here only so the tests can construct
    // records without reaching into read-fonts internals.
    const FLAG_ON_CURVE: u8 = 0x01;
    const FLAG_REPEAT: u8 = 0x08;

    /// Hand-encode a triangle so the test doesn't depend on fontTools.
    fn triangle_bytes() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
        v.extend_from_slice(&100i16.to_be_bytes()); // xMin
        v.extend_from_slice(&100i16.to_be_bytes()); // yMin
        v.extend_from_slice(&900i16.to_be_bytes()); // xMax
        v.extend_from_slice(&900i16.to_be_bytes()); // yMax
        v.extend_from_slice(&2u16.to_be_bytes()); // endPtsOfContours
        v.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
        v.push(FLAG_ON_CURVE);
        v.push(FLAG_ON_CURVE);
        v.push(FLAG_ON_CURVE);
        // X deltas: 500, -400, 800 (signed 16-bit each).
        v.extend_from_slice(&500i16.to_be_bytes());
        v.extend_from_slice(&(-400i16).to_be_bytes());
        v.extend_from_slice(&800i16.to_be_bytes());
        // Y deltas: 900, -800, 0.
        v.extend_from_slice(&900i16.to_be_bytes());
        v.extend_from_slice(&(-800i16).to_be_bytes());
        v.extend_from_slice(&0i16.to_be_bytes());
        v
    }

    #[test]
    fn decodes_triangle() {
        let out = decode(&triangle_bytes()).unwrap();
        assert_eq!(out.contours.len(), 1);
        let c = &out.contours[0];
        assert_eq!(c.len(), 3);
        assert_eq!(
            c[0],
            Point {
                x: 500,
                y: 900,
                on_curve: true
            }
        );
        assert_eq!(
            c[1],
            Point {
                x: 100,
                y: 100,
                on_curve: true
            }
        );
        assert_eq!(
            c[2],
            Point {
                x: 900,
                y: 100,
                on_curve: true
            }
        );
    }

    #[test]
    fn rejects_composite() {
        let mut v = Vec::new();
        v.extend_from_slice(&(-1i16).to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        assert_eq!(decode(&v), Err(DecodeError::Composite));
    }

    #[test]
    fn rejects_hinting() {
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
        v.extend_from_slice(&[0u8; 8]); // bounding box
        v.extend_from_slice(&0u16.to_be_bytes()); // endPtsOfContours[0] = 0
        v.extend_from_slice(&1u16.to_be_bytes()); // instructionLength = 1
        v.push(0x00); // one instruction byte — enough to trip the check
        v.push(FLAG_ON_CURVE); // single on-curve point
        v.extend_from_slice(&0i16.to_be_bytes()); // x delta
        v.extend_from_slice(&0i16.to_be_bytes()); // y delta
        assert_eq!(decode(&v), Err(DecodeError::Hinted));
    }

    #[test]
    fn rejects_truncated() {
        assert_eq!(decode(&[]), Err(DecodeError::Malformed));
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        // No contour data at all — header only.
        assert_eq!(decode(&v), Err(DecodeError::Malformed));
    }

    #[test]
    fn handles_repeat_flag() {
        // 4 points with identical flags, encoded via REPEAT.
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v.extend_from_slice(&3u16.to_be_bytes()); // endPtsOfContours
        v.extend_from_slice(&0u16.to_be_bytes()); // instructionLength
        v.push(FLAG_ON_CURVE | FLAG_REPEAT);
        v.push(3); // repeat this flag 3 more times → 4 points total
        for dx in [10i16, 10, 10, 10] {
            v.extend_from_slice(&dx.to_be_bytes());
        }
        for dy in [0i16, 10, 0, -10] {
            v.extend_from_slice(&dy.to_be_bytes());
        }
        let out = decode(&v).unwrap();
        assert_eq!(out.contours[0].len(), 4);
        assert_eq!(out.contours[0][3].x, 40);
        assert_eq!(out.contours[0][3].y, 0);
    }

    #[test]
    fn walk_triangle_produces_move_line_line_close() {
        let out = decode(&triangle_bytes()).unwrap();
        let cmds = out.walk(1000, 100.0);
        assert!(matches!(cmds[0], PathCmd::MoveTo { .. }));
        assert_eq!(cmds.len(), 4);
        assert!(matches!(cmds[1], PathCmd::LineTo { .. }));
        assert!(matches!(cmds[2], PathCmd::LineTo { .. }));
        assert!(matches!(cmds[3], PathCmd::Close));
    }

    #[test]
    fn walk_handles_two_off_curves_in_a_row() {
        // 4-point contour: on, off, off, on. Adjacent off-curves imply
        // an on-curve at their midpoint.
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v.extend_from_slice(&3u16.to_be_bytes());
        v.extend_from_slice(&0u16.to_be_bytes());
        v.push(FLAG_ON_CURVE);
        v.push(0);
        v.push(0);
        v.push(FLAG_ON_CURVE);
        for dx in [0i16, 100, 100, 0] {
            v.extend_from_slice(&dx.to_be_bytes());
        }
        for dy in [0i16, 100, -100, -100] {
            v.extend_from_slice(&dy.to_be_bytes());
        }
        let out = decode(&v).unwrap();
        let cmds = out.walk(1000, 1000.0);
        assert!(cmds.iter().any(|c| matches!(c, PathCmd::QuadTo { .. })));
        assert!(matches!(cmds[0], PathCmd::MoveTo { .. }));
        assert!(matches!(cmds.last().unwrap(), PathCmd::Close));
    }

    #[test]
    fn scale_and_flip_are_correct() {
        let out = decode(&triangle_bytes()).unwrap();
        let cmds = out.walk(1000, 100.0);
        // First on-curve is (500,900) in glyf space. Scaled: 500 * 0.1 = 50,
        // y_base = 900 * 0.1 = 90, flipped y = 90 - 90 = 0.
        if let PathCmd::MoveTo { x, y } = cmds[0] {
            assert!((x - 50.0).abs() < 1e-3);
            assert!((y - 0.0).abs() < 1e-3);
        } else {
            panic!("expected MoveTo");
        }
    }
}
