// CPU rasteriser for OpenType COLR v0 / v1 paint graphs.
//
// Drives ttf-parser's `colr::Painter` trait against a `tiny-skia`
// backend. The result is an RGBA8 bitmap that the caller uploads
// into sugarloaf's colour atlas — so the same code path works across
// all three backends (Wgpu, native Metal, Cpu) since rasterisation
// happens on CPU and only the upload is backend-specific.
//
// Correctness budget:
//   * Linear + radial gradients are handled correctly, including the
//     3-point → 2-point projection COLR v1 requires (porting the
//     math from skrifa/color/traversal.rs).
//   * Sweep gradients degrade to the first stop's solid colour.
//   * Variable-font coordinates render at the default instance.
//   * Composite modes beyond the painter's-algorithm `SrcOver` map
//     through `CompositeMode → BlendMode`; tiny-skia implements the
//     full set so there's no loss, but we pass them through rather
//     than validating each one per font.
//
// Rasterisation is cached per `(codepoint, pixel_size)` upstream by
// the existing glyph cache, so a 1-2 ms CPU pass per unique glyph
// size is invisible to the user.

use ttf_parser::colr::{
    ClipBox, CompositeMode, GradientExtend, LinearGradient as TtfLinear, Paint, Painter,
    RadialGradient as TtfRadial,
};
use ttf_parser::{GlyphId, RgbaColor, Transform as TtfTransform};

use tiny_skia::{
    BlendMode, Color, FillRule, GradientStop, LinearGradient, Mask, Paint as SkPaint,
    Path, PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient, Rect, Shader,
    SpreadMode, Transform,
};

use crate::font::glyf_decode;
use crate::font::glyph_placement::{
    place, CellGeometry, PlacedTransform, PlacementParams,
};

/// Hard ceiling on either bitmap axis. The §8.5 size modes allow the
/// scaled outline to overflow its span (`height`, `advance`, `cover`),
/// but a hostile registration (`aw=1` + `size=advance`) could otherwise
/// request a gigapixel pixmap. Anything past this bound wouldn't fit an
/// atlas page anyway.
const MAX_RASTER_DIM: f32 = 4096.0;

/// Bitmap + placement metadata produced by [`rasterize_payload`] (and
/// internally by [`rasterize`]). `is_color` distinguishes the two
/// underlying formats: `false` → A8 alpha mask (mono `glyf`), `true`
/// → straight RGBA8 (`colrv0` / `colrv1`). The grid renderer routes
/// mono entries to the grayscale atlas and colour entries to the
/// colour atlas based on this flag.
pub struct RasterizedPayload {
    pub data: Vec<u8>,
    pub width: u16,
    pub height: u16,
    /// Pixel offset from the cell's LEFT edge to the bitmap's left
    /// edge. May be negative when the placed outline overflows the
    /// cell (e.g. `size=height` with a wide extent).
    pub left: i32,
    /// Pixel offset from the cell's TOP edge to the bitmap's top edge
    /// (top-down). The §8.5 placement model already positioned the
    /// outline inside the cell box, so no baseline math is left for
    /// the caller — converting to the grid's bottom-up bearing is
    /// `cell_height - top`.
    pub top: i32,
    pub is_color: bool,
}

/// Rasterise a registered Glyph Protocol payload, applying the §8.5
/// pad → size → align placement model against the target cell box.
/// Dispatches the monochrome `glyf` path through tiny-skia's
/// anti-aliased fill (A8 output) and the colour `colrv0`/`colrv1`
/// paths through the COLR painter graph (RGBA8 output). Returns `None`
/// on malformed payload or degenerate sizing.
pub fn rasterize_payload(
    payload: &crate::font::glyph_registry::StoredPayload,
    placement: &PlacementParams,
    geom: &CellGeometry,
    foreground_rgba: [u8; 4],
) -> Option<RasterizedPayload> {
    use crate::font::glyph_registry::StoredPayload;
    match payload {
        StoredPayload::Glyf { glyf } => rasterize_mono(glyf, placement, geom),
        StoredPayload::ColrV0 { glyphs, colr, cpal }
        | StoredPayload::ColrV1 { glyphs, colr, cpal } => {
            rasterize(glyphs, colr, cpal, placement, geom, foreground_rgba)
        }
    }
}

/// The placed bitmap rectangle for a design-space bbox: cell-local
/// `left`/`top` plus pixel dimensions, padded 1 px on each side so
/// anti-aliased edges never clip (sub-pixel expansion via
/// floor/ceil). `None` when the transform degenerates or the result
/// exceeds [`MAX_RASTER_DIM`].
fn placed_bitmap_rect(
    t: &PlacedTransform,
    x_min: f32,
    y_min: f32,
    x_max: f32,
    y_max: f32,
) -> Option<(f32, f32, u32, u32)> {
    if !(t.sx.is_finite() && t.sy.is_finite()) || t.sx <= 0.0 || t.sy <= 0.0 {
        return None;
    }
    let pad = 1.0_f32;
    // Y flips: design y_max is the bitmap top.
    let left = (t.map_x(x_min) - pad).floor();
    let top = (t.map_y(y_max) - pad).floor();
    let w = (t.map_x(x_max) + pad).ceil() - left;
    let h = (t.map_y(y_min) + pad).ceil() - top;
    if !(left.is_finite() && top.is_finite()) || w > MAX_RASTER_DIM || h > MAX_RASTER_DIM
    {
        return None;
    }
    Some((left, top, w.max(1.0) as u32, h.max(1.0) as u32))
}

/// Walk a `glyf` simple-glyph outline and rasterise it as an A8 alpha
/// mask at its §8.5-placed size. The atlas-bound caller uploads the
/// bytes straight into the grayscale atlas, same shape as the
/// swash/CT mono path produces.
fn rasterize_mono(
    glyf: &[u8],
    placement: &PlacementParams,
    geom: &CellGeometry,
) -> Option<RasterizedPayload> {
    let outline = glyf_decode::decode(glyf).ok()?;
    let t = place(placement, geom, outline.y_min as f32);
    let (left, top, pix_w, pix_h) = placed_bitmap_rect(
        &t,
        outline.x_min as f32,
        outline.y_min as f32,
        outline.x_max as f32,
        outline.y_max as f32,
    )?;

    // `glyf_decode::Outline::walk(1, 1.0)` emits design units with Y
    // already flipped down from the bbox top: `y_walk = y_max − y`.
    // Composing with the placement transform:
    //   px_x = t.map_x(x_walk)
    //   px_y = t.map_y(y) = (t.ty − sy·y_max) + sy·y_walk
    // then shift into bitmap-local space by (−left, −top).
    let cmds = outline.walk(1, 1.0);
    if cmds.is_empty() {
        return None;
    }
    let mut pb = PathBuilder::new();
    for cmd in &cmds {
        match *cmd {
            glyf_decode::PathCmd::MoveTo { x, y } => pb.move_to(x, y),
            glyf_decode::PathCmd::LineTo { x, y } => pb.line_to(x, y),
            glyf_decode::PathCmd::QuadTo { cx, cy, x, y } => pb.quad_to(cx, cy, x, y),
            glyf_decode::PathCmd::Close => pb.close(),
        }
    }
    let path = pb.finish()?;

    let mut pixmap = Pixmap::new(pix_w, pix_h)?;
    let ctm = Transform::from_row(
        t.sx,
        0.0,
        0.0,
        t.sy,
        t.tx - left,
        (t.ty - t.sy * outline.y_max as f32) - top,
    );
    let mut paint = SkPaint::default();
    paint.set_color_rgba8(0xFF, 0xFF, 0xFF, 0xFF);
    paint.anti_alias = true;
    pixmap.fill_path(&path, &paint, FillRule::Winding, ctm, None);

    // Pixmap stores premultiplied RGBA; for an A8 mask we just take
    // the alpha channel (which equals R/G/B since we filled white).
    let data: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();

    Some(RasterizedPayload {
        data,
        width: pix_w as u16,
        height: pix_h as u16,
        left: left as i32,
        top: top as i32,
        is_color: false,
    })
}

/// Rasterise a COLR glyph to RGBA at its §8.5-placed size. Returns
/// `None` when COLR/CPAL is malformed, when the base-glyph outline is
/// empty, or when tiny-skia rejects a degenerate configuration (e.g.
/// zero pixmap size).
pub(super) fn rasterize(
    glyphs: &[Vec<u8>],
    colr_bytes: &[u8],
    cpal_bytes: &[u8],
    placement: &PlacementParams,
    geom: &CellGeometry,
    foreground: [u8; 4],
) -> Option<RasterizedPayload> {
    // ttf-parser's `colr::Table::parse` requires a non-empty CPAL
    // slice, even for v1 fonts that make no palette lookups. If the
    // container ships an empty CPAL (legal for v1-only paints), feed
    // the parser a zero-entry placeholder.
    const EMPTY_CPAL: [u8; 12] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0C,
    ];
    let cpal_source: &[u8] = if cpal_bytes.is_empty() {
        &EMPTY_CPAL
    } else {
        cpal_bytes
    };
    let cpal = ttf_parser::cpal::Table::parse(cpal_source)?;
    let colr = ttf_parser::colr::Table::parse(cpal, colr_bytes)?;
    let base_gid = first_base_glyph_id(colr_bytes, glyphs)?;

    // Prefer the COLR ClipBox — authoritative per the OpenType spec,
    // and required for emoji fonts (Noto Color Emoji, etc.) whose base
    // glyphs are empty wrappers that reference layer glyphs via the
    // paint graph. Fall back to the base glyph's `glyf` bbox for fonts
    // like Nabla that carry geometry on the base glyph. Pad 1 px each
    // side so anti-aliased layer edges that drift slightly past the
    // declared bbox (common in hand-authored fonts) aren't clipped.
    //
    // Widened to i32 immediately because a saturated ClipBox (e.g.
    // `x_min = i16::MIN`, `x_max = i16::MAX`) would overflow on the
    // `x_max - x_min` subtraction below if kept as i16 — wrapping in
    // release and panicking in debug.
    let (x_min, y_min, x_max, y_max): (i32, i32, i32, i32) =
        match colr.clip_box(GlyphId(base_gid), &[]) {
            Some(cb) => (
                cb.x_min.floor() as i32,
                cb.y_min.floor() as i32,
                cb.x_max.ceil() as i32,
                cb.y_max.ceil() as i32,
            ),
            None => {
                let (a, b, c, d) = glyf_bbox(glyphs.get(base_gid as usize)?)?;
                (a as i32, b as i32, c as i32, d as i32)
            }
        };

    let t = place(placement, geom, y_min as f32);
    let (left, top, pix_w, pix_h) =
        placed_bitmap_rect(&t, x_min as f32, y_min as f32, x_max as f32, y_max as f32)?;

    let base_pixmap = Pixmap::new(pix_w, pix_h)?;

    // Font units (y-up, origin at baseline) → bitmap-local pixels
    // (y-down, origin at the bitmap's top-left). The placement
    // transform maps to cell-local pixels; subtracting the bitmap
    // origin shifts into bitmap space.
    // Matrix [sx, ky, kx, sy, tx, ty] applies (x,y) as
    //   (sx*x + kx*y + tx, ky*x + sy*y + ty).
    let base_ctm = Transform::from_row(t.sx, 0.0, 0.0, -t.sy, t.tx - left, t.ty - top);

    let mut raster = ColorRaster {
        layers: vec![Layer {
            pixmap: base_pixmap,
            mode: CompositeMode::SourceOver,
        }],
        transforms: vec![base_ctm],
        clips: vec![None],
        current_path: None,
        glyphs,
    };

    let fg = RgbaColor::new(foreground[0], foreground[1], foreground[2], foreground[3]);
    colr.paint(GlyphId(base_gid), 0, &mut raster, &[], fg)?;

    debug_assert_eq!(raster.layers.len(), 1, "layer stack should drain");
    let final_pixmap = raster.layers.pop().unwrap().pixmap;

    Some(RasterizedPayload {
        data: pixmap_to_rgba(&final_pixmap),
        width: pix_w as u16,
        height: pix_h as u16,
        left: left as i32,
        top: top as i32,
        is_color: true,
    })
}

struct Layer {
    pixmap: Pixmap,
    mode: CompositeMode,
}

struct ColorRaster<'a> {
    layers: Vec<Layer>,
    transforms: Vec<Transform>,
    clips: Vec<Option<Mask>>,
    current_path: Option<Path>,
    glyphs: &'a [Vec<u8>],
}

impl ColorRaster<'_> {
    fn top_ctm(&self) -> Transform {
        *self.transforms.last().unwrap_or(&Transform::identity())
    }

    fn top_clip(&self) -> Option<&Mask> {
        self.clips.last().and_then(|c| c.as_ref())
    }

    fn top_pixmap(&mut self) -> &mut Pixmap {
        &mut self.layers.last_mut().unwrap().pixmap
    }

    fn fill_current(&mut self, paint: SkPaint) {
        let Some(path) = self.current_path.clone() else {
            return;
        };
        let ctm = self.top_ctm();
        // Clone the clip mask so we can borrow the pixmap mutably.
        let clip = self.top_clip().cloned();
        let pixmap = self.top_pixmap();
        pixmap.fill_path(&path, &paint, FillRule::Winding, ctm, clip.as_ref());
    }
}

impl<'a> Painter<'a> for ColorRaster<'a> {
    fn outline_glyph(&mut self, glyph_id: GlyphId) {
        let idx = glyph_id.0 as usize;
        let Some(bytes) = self.glyphs.get(idx) else {
            self.current_path = None;
            return;
        };
        if bytes.is_empty() {
            self.current_path = None;
            return;
        }
        self.current_path = build_path(bytes);
    }

    fn paint(&mut self, paint: Paint<'a>) {
        match paint {
            Paint::Solid(color) => {
                let p = SkPaint {
                    shader: Shader::SolidColor(rgba_to_color(color)),
                    anti_alias: true,
                    ..SkPaint::default()
                };
                self.fill_current(p);
            }
            Paint::LinearGradient(lg) => {
                if let Some(shader) = linear_gradient_shader(&lg) {
                    let p = SkPaint {
                        shader,
                        anti_alias: true,
                        ..SkPaint::default()
                    };
                    self.fill_current(p);
                }
            }
            Paint::RadialGradient(rg) => {
                if let Some(shader) = radial_gradient_shader(&rg) {
                    let p = SkPaint {
                        shader,
                        anti_alias: true,
                        ..SkPaint::default()
                    };
                    self.fill_current(p);
                }
            }
            Paint::SweepGradient(sg) => {
                // Sweep gradients don't map to tiny-skia (no sweep
                // shader). Degrade to the first stop's solid colour.
                // Nabla doesn't use sweeps; extending later means
                // writing a custom per-pixel shader.
                if let Some(first) = sg.stops(0, &[]).next() {
                    let p = SkPaint {
                        shader: Shader::SolidColor(rgba_to_color(first.color)),
                        anti_alias: true,
                        ..SkPaint::default()
                    };
                    self.fill_current(p);
                }
            }
        }
    }

    fn push_clip(&mut self) {
        let Some(path) = self.current_path.clone() else {
            // Keep the stack height balanced for the matching pop.
            self.clips.push(self.top_clip().cloned());
            return;
        };
        let ctm = self.top_ctm();
        let parent = self.top_clip().cloned();
        let (pw, ph) = {
            let p = self.top_pixmap();
            (p.width(), p.height())
        };
        let Some(mut mask) = Mask::new(pw, ph) else {
            self.clips.push(parent);
            return;
        };
        mask.fill_path(&path, FillRule::Winding, true, ctm);
        if let Some(par) = parent {
            intersect_masks(&mut mask, &par);
        }
        self.clips.push(Some(mask));
    }

    fn push_clip_box(&mut self, clipbox: ClipBox) {
        let parent = self.top_clip().cloned();
        let (pw, ph) = {
            let p = self.top_pixmap();
            (p.width(), p.height())
        };
        let Some(rect) =
            Rect::from_ltrb(clipbox.x_min, clipbox.y_min, clipbox.x_max, clipbox.y_max)
        else {
            self.clips.push(parent);
            return;
        };
        let path = PathBuilder::from_rect(rect);
        let ctm = self.top_ctm();
        let Some(mut mask) = Mask::new(pw, ph) else {
            self.clips.push(parent);
            return;
        };
        mask.fill_path(&path, FillRule::Winding, true, ctm);
        if let Some(par) = parent {
            intersect_masks(&mut mask, &par);
        }
        self.clips.push(Some(mask));
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        let (w, h) = {
            let base = self.top_pixmap();
            (base.width(), base.height())
        };
        let Some(pixmap) = Pixmap::new(w, h) else {
            // Out of memory — push a token entry so pop_layer stays
            // balanced. Drawing will fail silently until the pop.
            self.layers.push(Layer {
                pixmap: Pixmap::new(1, 1).unwrap(),
                mode,
            });
            self.clips.push(self.top_clip().cloned());
            return;
        };
        self.layers.push(Layer { pixmap, mode });
        // Layers inherit the enclosing clip. Every push_layer is
        // paired with a pop_layer; we push a matching clip entry so
        // the stack heights stay in lock-step.
        self.clips.push(self.top_clip().cloned());
    }

    fn pop_layer(&mut self) {
        let Some(top) = self.layers.pop() else { return };
        self.clips.pop();
        let blend = composite_mode_to_blend(top.mode);
        let Some(parent) = self.layers.last_mut() else {
            // Stack imbalance — should be unreachable given
            // ttf-parser's own push/pop pairing.
            return;
        };
        parent.pixmap.draw_pixmap(
            0,
            0,
            top.pixmap.as_ref(),
            &PixmapPaint {
                opacity: 1.0,
                blend_mode: blend,
                quality: tiny_skia::FilterQuality::Nearest,
            },
            Transform::identity(),
            None,
        );
    }

    fn push_transform(&mut self, transform: TtfTransform) {
        let t = Transform::from_row(
            transform.a,
            transform.b,
            transform.c,
            transform.d,
            transform.e,
            transform.f,
        );
        let ctm = self.top_ctm().pre_concat(t);
        self.transforms.push(ctm);
    }

    fn pop_transform(&mut self) {
        self.transforms.pop();
    }
}

/// Parse the COLR header's base-glyph records and return the first
/// one whose outline slot in `glyphs` is non-empty.
///
/// Naive "take record 0" doesn't work: fontTools sorts `BaseGlyphList`
/// by glyphID and keeps a `BaseGlyphPaintRecord` for `.notdef` (GID
/// 0), which has an empty outline after subsetting. We need to skip
/// past those empty slots and find the first record that actually
/// has ink. Prefers v1's `BaseGlyphList`, falls back to v0's
/// `BaseGlyphRecord` array.
fn first_base_glyph_id(colr: &[u8], glyphs: &[Vec<u8>]) -> Option<u16> {
    if colr.len() < 8 {
        return None;
    }
    let is_non_empty =
        |gid: u16| -> bool { glyphs.get(gid as usize).is_some_and(|g| !g.is_empty()) };

    // v1 BaseGlyphList: u32 numRecords, then records of
    // { u16 glyphID, u32 paintOffset } = 6 bytes each.
    if colr.len() >= 18 {
        let v1_off =
            u32::from_be_bytes([colr[14], colr[15], colr[16], colr[17]]) as usize;
        if v1_off != 0 && v1_off + 4 <= colr.len() {
            let num_records = u32::from_be_bytes([
                colr[v1_off],
                colr[v1_off + 1],
                colr[v1_off + 2],
                colr[v1_off + 3],
            ]) as usize;
            let mut first_gid = None;
            for i in 0..num_records {
                let rec_off = v1_off + 4 + i * 6;
                if rec_off + 2 > colr.len() {
                    break;
                }
                let gid = u16::from_be_bytes([colr[rec_off], colr[rec_off + 1]]);
                first_gid.get_or_insert(gid);
                if is_non_empty(gid) {
                    return Some(gid);
                }
            }
            // No non-empty record found. Return the first one we saw
            // so the caller still has something; the subsequent bbox
            // read will bail cleanly.
            if let Some(g) = first_gid {
                return Some(g);
            }
        }
    }

    // v0 BaseGlyphRecord array: { u16 glyphID, u16 firstLayer, u16 numLayers } = 6 B.
    let num_v0 = u16::from_be_bytes([colr[2], colr[3]]) as usize;
    let v0_off = u32::from_be_bytes([colr[4], colr[5], colr[6], colr[7]]) as usize;
    let mut first_gid = None;
    for i in 0..num_v0 {
        let rec_off = v0_off + i * 6;
        if rec_off + 2 > colr.len() {
            break;
        }
        let gid = u16::from_be_bytes([colr[rec_off], colr[rec_off + 1]]);
        first_gid.get_or_insert(gid);
        if is_non_empty(gid) {
            return Some(gid);
        }
    }
    first_gid
}

fn glyf_bbox(bytes: &[u8]) -> Option<(i16, i16, i16, i16)> {
    if bytes.len() < 10 {
        return None;
    }
    let xmin = i16::from_be_bytes([bytes[2], bytes[3]]);
    let ymin = i16::from_be_bytes([bytes[4], bytes[5]]);
    let xmax = i16::from_be_bytes([bytes[6], bytes[7]]);
    let ymax = i16::from_be_bytes([bytes[8], bytes[9]]);
    Some((xmin, ymin, xmax, ymax))
}

/// Decode a glyf simple-glyph record into an unscaled, Y-up
/// design-unit `Path`. The painter's CTM is responsible for the
/// scale + Y-flip at draw time.
///
/// `glyf_decode::Outline::walk` already flips Y so its output sits
/// in Y-down origin-at-y_max space. For the COLR painter we want
/// pristine design-unit Y-up coordinates (so paint-graph transforms
/// compose correctly), so we un-flip walk's output by subtracting
/// from `y_max`. Equivalent to a dedicated y-preserving walker, but
/// reuses `walk`'s existing implied-on-curve handling.
fn build_path(bytes: &[u8]) -> Option<Path> {
    let outline = glyf_decode::decode(bytes).ok()?;
    let y_max = outline.y_max as f32;
    // `walk(upm=1, size=1.0)` gives us an identity scale, so every
    // coord out is `design_x, y_max - design_y`. Un-flip Y below.
    let cmds = outline.walk(1, 1.0);
    if cmds.is_empty() {
        return None;
    }
    let unflip = |y: f32| y_max - y;
    let mut pb = PathBuilder::new();
    for cmd in &cmds {
        match *cmd {
            glyf_decode::PathCmd::MoveTo { x, y } => pb.move_to(x, unflip(y)),
            glyf_decode::PathCmd::LineTo { x, y } => pb.line_to(x, unflip(y)),
            glyf_decode::PathCmd::QuadTo { cx, cy, x, y } => {
                pb.quad_to(cx, unflip(cy), x, unflip(y))
            }
            glyf_decode::PathCmd::Close => pb.close(),
        }
    }
    pb.finish()
}

/// tiny-skia Pixmap pixels are premultiplied. The v4 grid color atlas
/// expects premultiplied RGBA — its Metal/wgpu/vulkan pipelines all
/// configure source-blend = `One`, dest-blend = `OneMinusSourceAlpha`,
/// matching `MTLSamplerAddressMode`/system-emoji rasteriser conventions.
/// So pass the bytes through verbatim. (The previous PR plumbed COLR
/// glyphs through the rich-text image-cache, whose pipeline used
/// `SourceAlpha + OneMinusSourceAlpha` and therefore wanted straight
/// alpha — that path no longer exists in main.)
fn pixmap_to_rgba(pixmap: &Pixmap) -> Vec<u8> {
    let pixels = pixmap.pixels();
    let mut out = Vec::with_capacity(pixels.len() * 4);
    for p in pixels {
        out.push(p.red());
        out.push(p.green());
        out.push(p.blue());
        out.push(p.alpha());
    }
    out
}

fn rgba_to_color(c: RgbaColor) -> Color {
    Color::from_rgba8(c.red, c.green, c.blue, c.alpha)
}

/// Collect + sort COLR stops. Returns the raw `(offset, color)`
/// pairs so the caller can still pull the first stop's colour for
/// single-stop degeneracy (tiny-skia's `GradientStop` fields are
/// `pub(crate)` and don't expose the colour back).
fn collect_stops(
    iter: impl Iterator<Item = ttf_parser::colr::ColorStop>,
) -> Vec<(f32, Color)> {
    let mut stops: Vec<(f32, Color)> = iter
        .map(|s| (s.stop_offset, rgba_to_color(s.color)))
        .collect();
    stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    stops
}

fn stops_to_tiny_skia(stops: &[(f32, Color)]) -> Vec<GradientStop> {
    stops
        .iter()
        .map(|&(o, c)| GradientStop::new(o, c))
        .collect()
}

fn extend_to_spread(e: GradientExtend) -> SpreadMode {
    match e {
        GradientExtend::Pad => SpreadMode::Pad,
        GradientExtend::Repeat => SpreadMode::Repeat,
        GradientExtend::Reflect => SpreadMode::Reflect,
    }
}

/// Project COLR's 3-point linear-gradient form to the 2-point form
/// tiny-skia wants. Returns `P3`, the point on the perpendicular to
/// `P0→P2` through `P0` that corresponds to `P1`.
///
/// Two equivalent formulations of the same geometry are in the wild:
///
/// - **skrifa/nanoemoji**: `P3 = P0 + project(P1 - P0, perp(P2 - P0))`
///   — project onto the perpendicular axis, add back to P0.
/// - **This impl**: `P3 = P1 - t * (P2 - P0)` where
///   `t = ((P1 - P0) · (P2 - P0)) / |P2 - P0|²`
///   — remove the parallel component from `(P1 - P0)` and add P0.
///
/// Algebraically these give the same point: subtracting the parallel
/// component of `(P1 - P0)` leaves its perpendicular component, and
/// `P0 + perp_component = P1 - parallel_component`. The FreeType
/// COLRv1 reference implementation uses the second formulation.
///
/// Returns `None` if `P0 == P2` (degenerate axis with no direction).
fn project_p3(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32)) -> Option<(f32, f32)> {
    let dx = p2.0 - p0.0;
    let dy = p2.1 - p0.1;
    let len_sq = dx * dx + dy * dy;
    if !len_sq.is_finite() || len_sq < 1e-6 {
        return None;
    }
    let bx = p1.0 - p0.0;
    let by = p1.1 - p0.1;
    let t = (bx * dx + by * dy) / len_sq;
    Some((p1.0 - t * dx, p1.1 - t * dy))
}

/// Build a tiny-skia linear-gradient shader for a COLR linear paint.
/// The 3-point → 2-point projection happens via [`project_p3`].
///
/// Stop normalisation (extending the P0-P3 line when stops sit
/// outside `[0, 1]`) is NOT performed: tiny-skia clamps stop offsets
/// to `[0, 1]`, so a gradient with stops at e.g. `-0.2`..`1.2`
/// renders truncated at the boundaries. Nabla's stops sit within
/// `[0, 1]` so this hasn't bitten the demo. Handling wide stops
/// would mean moving `P0` and `P3` outward by the offset overhang
/// and rescaling stops to fit `[0, 1]`; skrifa's traversal.rs has
/// the full math.
fn linear_gradient_shader(lg: &TtfLinear<'_>) -> Option<Shader<'static>> {
    let p0 = (lg.x0, lg.y0);
    let p1 = (lg.x1, lg.y1);
    let p2 = (lg.x2, lg.y2);
    let (p3x, p3y) = project_p3(p0, p1, p2)?;

    let stops = collect_stops(lg.stops(0, &[]));
    if stops.len() < 2 {
        return stops.into_iter().next().map(|(_, c)| Shader::SolidColor(c));
    }

    LinearGradient::new(
        Point::from_xy(p0.0, p0.1),
        Point::from_xy(p3x, p3y),
        stops_to_tiny_skia(&stops),
        extend_to_spread(lg.extend),
        Transform::identity(),
    )
}

fn radial_gradient_shader(rg: &TtfRadial<'_>) -> Option<Shader<'static>> {
    let stops = collect_stops(rg.stops(0, &[]));
    if stops.len() < 2 {
        return stops.into_iter().next().map(|(_, c)| Shader::SolidColor(c));
    }
    RadialGradient::new(
        Point::from_xy(rg.x0, rg.y0),
        rg.r0.max(0.0),
        Point::from_xy(rg.x1, rg.y1),
        rg.r1.max(0.1),
        stops_to_tiny_skia(&stops),
        extend_to_spread(rg.extend),
        Transform::identity(),
    )
}

fn composite_mode_to_blend(mode: CompositeMode) -> BlendMode {
    use CompositeMode::*;
    match mode {
        Clear => BlendMode::Clear,
        Source => BlendMode::Source,
        Destination => BlendMode::Destination,
        SourceOver => BlendMode::SourceOver,
        DestinationOver => BlendMode::DestinationOver,
        SourceIn => BlendMode::SourceIn,
        DestinationIn => BlendMode::DestinationIn,
        SourceOut => BlendMode::SourceOut,
        DestinationOut => BlendMode::DestinationOut,
        SourceAtop => BlendMode::SourceAtop,
        DestinationAtop => BlendMode::DestinationAtop,
        Xor => BlendMode::Xor,
        Plus => BlendMode::Plus,
        Screen => BlendMode::Screen,
        Overlay => BlendMode::Overlay,
        Darken => BlendMode::Darken,
        Lighten => BlendMode::Lighten,
        ColorDodge => BlendMode::ColorDodge,
        ColorBurn => BlendMode::ColorBurn,
        HardLight => BlendMode::HardLight,
        SoftLight => BlendMode::SoftLight,
        Difference => BlendMode::Difference,
        Exclusion => BlendMode::Exclusion,
        Multiply => BlendMode::Multiply,
        Hue => BlendMode::Hue,
        Saturation => BlendMode::Saturation,
        Color => BlendMode::Color,
        Luminosity => BlendMode::Luminosity,
    }
}

/// Intersect two 8-bit alpha masks in place: `dst = dst ∩ src`.
/// Used when pushing nested clips — the new clip region is the
/// logical intersection of the outer and inner paths. Both masks
/// share dimensions by construction (we always build them from
/// the current pixmap's size).
fn intersect_masks(dst: &mut Mask, src: &Mask) {
    if dst.width() != src.width() || dst.height() != src.height() {
        return;
    }
    let dst_bytes = dst.data_mut();
    let src_bytes = src.data();
    for (d, &s) in dst_bytes.iter_mut().zip(src_bytes.iter()) {
        *d = ((*d as u16 * s as u16) / 255) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: vector dot product in 2D.
    fn dot(a: (f32, f32), b: (f32, f32)) -> f32 {
        a.0 * b.0 + a.1 * b.1
    }

    /// Build a `glyf` simple-glyph whose bbox is the full `em_top × em_top`
    /// square but whose inked region is just the top `strip_height` rows
    /// (design Y from `em_top - strip_height` to `em_top`). The bbox in
    /// the glyf header is authoritative — `glyf_decode` reads it directly
    /// — so the rasterised pixmap will be em_top×em_top pixels with only
    /// the top strip filled. That's what lets the test distinguish "top"
    /// from "bottom" of the bitmap.
    fn glyf_top_strip(em_top: i16, strip_height: i16) -> Vec<u8> {
        // glyf simple-glyph layout per OpenType:
        //   i16 numberOfContours
        //   i16 xMin, yMin, xMax, yMax  (authoritative bbox — NOT derived from points)
        //   u16 endPtsOfContours[numContours]
        //   u16 instructionLength
        //   u8  flags[numPoints]
        //   coords (deltas, big-endian i16 when not using shorts)
        let strip_bottom = em_top - strip_height;
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
                                                  // Declare bbox as the full em — not just the inked strip — so
                                                  // the rasterised pixmap has empty space below the strip we
                                                  // can sample as "bottom".
        v.extend_from_slice(&0i16.to_be_bytes()); // xMin
        v.extend_from_slice(&0i16.to_be_bytes()); // yMin
        v.extend_from_slice(&em_top.to_be_bytes()); // xMax
        v.extend_from_slice(&em_top.to_be_bytes()); // yMax
        v.extend_from_slice(&3u16.to_be_bytes()); // endPtsOfContours[0] = 3 (4 points)
        v.extend_from_slice(&0u16.to_be_bytes()); // instructionLength = 0
        v.extend_from_slice(&[0x01; 4]); // 4 flags, all on-curve, full i16 deltas
                                         // Points walk the rectangle [0, strip_bottom] → [em_top, strip_bottom]
                                         // → [em_top, em_top] → [0, em_top]. Deltas from previous point
                                         // (first delta is from origin (0,0)).
        let xs = [0i16, em_top, 0, -em_top];
        let ys = [strip_bottom, 0, strip_height, 0];
        for x in &xs {
            v.extend_from_slice(&x.to_be_bytes());
        }
        for y in &ys {
            v.extend_from_slice(&y.to_be_bytes());
        }
        v
    }

    /// Square cell geometry sized to render the test glyph 1:1 when
    /// `size=height` maps `lh` onto the cell height.
    fn geom_cell(w: f32, h: f32) -> CellGeometry {
        CellGeometry {
            cell_width: w,
            cell_height: h,
            ascent: h * 0.8,
            span_cells: 1,
        }
    }

    #[test]
    fn rasterize_mono_top_pixels_filled_bottom_pixels_empty() {
        // Outline occupies only the top 25% of the em — after raster,
        // the top rows of the bitmap should be inked and the bottom
        // rows should be transparent. Catches the Y-flip bug we hit
        // earlier (where the strip rendered at the bottom instead).
        // A 100×100 cell with `lh=100` keeps the historical 1:1 scale.
        let upm = 100i16;
        let bytes = glyf_top_strip(upm, upm / 4);
        let r = rasterize_mono(
            &bytes,
            &PlacementParams::with_upm(upm as u16),
            &geom_cell(100.0, 100.0),
        )
        .expect("rasterize succeeds for valid simple glyph");

        let w = r.width as usize;
        let h = r.height as usize;
        assert!(w > 4 && h > 4, "bitmap should be larger than the padding");

        // Sample a row near the top (just below the 1-px pad) and a
        // row near the bottom. Use the centre column to avoid the
        // padding strip on the sides.
        let mid_x = w / 2;
        let top_y = 2;
        let bot_y = h - 2;
        let top_alpha = r.data[top_y * w + mid_x];
        let bot_alpha = r.data[bot_y * w + mid_x];

        assert!(
            top_alpha > 0,
            "top of bitmap should be inked (got alpha {top_alpha})"
        );
        assert!(
            bot_alpha == 0,
            "bottom of bitmap should be empty (got alpha {bot_alpha})"
        );
        assert!(!r.is_color, "glyf path produces an alpha mask");
    }

    #[test]
    fn rasterize_mono_rejects_degenerate_geometry() {
        let bytes = glyf_top_strip(100, 25);
        // Zero cell height → zero scale.
        assert!(rasterize_mono(
            &bytes,
            &PlacementParams::with_upm(100),
            &geom_cell(100.0, 0.0)
        )
        .is_none());
        // upm 0 clamps the extent to 1 design unit, blowing the scale
        // past MAX_RASTER_DIM — rejected rather than allocated.
        assert!(rasterize_mono(
            &bytes,
            &PlacementParams::with_upm(0),
            &geom_cell(100.0, 100.0)
        )
        .is_none());
    }

    #[test]
    fn rasterize_mono_contain_centers_inside_cell() {
        // Full-em square outline, contain-fit into a 10×20 cell: the
        // placed bitmap (minus the 1-px AA pad) must stay inside the
        // cell box on both axes.
        let bytes = glyf_top_strip(100, 100);
        let mut p = PlacementParams::with_upm(100);
        p.size = crate::font::glyph_placement::SizeMode::Contain;
        let r = rasterize_mono(&bytes, &p, &geom_cell(10.0, 20.0)).expect("rasterize");
        assert!(r.left >= -1, "left {} inside cell", r.left);
        assert!(r.top >= -1, "top {} inside cell", r.top);
        assert!(
            r.left + r.width as i32 <= 11,
            "right {} inside cell",
            r.left + r.width as i32
        );
        assert!(
            r.top + r.height as i32 <= 21,
            "bottom {} inside cell",
            r.top + r.height as i32
        );
    }

    #[test]
    fn rasterize_mono_stretch_fills_cell() {
        let bytes = glyf_top_strip(100, 100);
        let mut p = PlacementParams::with_upm(100);
        p.size = crate::font::glyph_placement::SizeMode::Stretch;
        let r = rasterize_mono(&bytes, &p, &geom_cell(10.0, 20.0)).expect("rasterize");
        // Cell box plus the 1-px AA pad on each side.
        assert_eq!(r.left, -1);
        assert_eq!(r.top, -1);
        assert_eq!(r.width, 12);
        assert_eq!(r.height, 22);
    }

    #[test]
    fn project_p3_is_perpendicular_to_p0p2_axis_through_p0() {
        // For any well-formed input, (P3 - P0) must be perpendicular
        // to (P2 - P0). This is the defining property of the
        // projection — skrifa, FreeType, and nanoemoji all document
        // it as the `P0-P3 ⟂ P0-P2` constraint.
        let cases = [
            ((0.0, 0.0), (10.0, 5.0), (20.0, 0.0)),
            ((100.0, 100.0), (150.0, 200.0), (200.0, 100.0)),
            ((0.0, 0.0), (3.0, 4.0), (5.0, 0.0)),
            ((-50.0, 25.0), (0.0, 75.0), (50.0, 25.0)),
        ];
        for (p0, p1, p2) in cases {
            let (p3x, p3y) = project_p3(p0, p1, p2).unwrap();
            let p0p3 = (p3x - p0.0, p3y - p0.1);
            let p0p2 = (p2.0 - p0.0, p2.1 - p0.1);
            let d = dot(p0p3, p0p2);
            assert!(
                d.abs() < 1e-3,
                "P0P3 · P0P2 = {d} for p0={p0:?} p1={p1:?} p2={p2:?}",
            );
        }
    }

    #[test]
    fn project_p3_matches_skrifa_formulation() {
        // Cross-check: P3 = P0 + project(P1-P0, perp(P2-P0)) should
        // give the same result as our formulation. Skrifa computes
        // this way; we compute P1 - t*(P2-P0). Both land on the same
        // point mathematically.
        let p0 = (10.0, 20.0);
        let p1 = (50.0, 80.0);
        let p2 = (100.0, 20.0);

        // Skrifa-style: project (P1-P0) onto perpendicular of (P2-P0).
        let perp_x = p2.1 - p0.1; // (dy, -dx) rotation of P0→P2
        let perp_y = -(p2.0 - p0.0);
        let b = (p1.0 - p0.0, p1.1 - p0.1);
        let perp_len_sq = perp_x * perp_x + perp_y * perp_y;
        let k = (b.0 * perp_x + b.1 * perp_y) / perp_len_sq;
        let skrifa_p3 = (p0.0 + k * perp_x, p0.1 + k * perp_y);

        let (our_p3x, our_p3y) = project_p3(p0, p1, p2).unwrap();
        assert!((our_p3x - skrifa_p3.0).abs() < 1e-3);
        assert!((our_p3y - skrifa_p3.1).abs() < 1e-3);
    }

    #[test]
    fn project_p3_rejects_degenerate_axis() {
        // P0 == P2 means the color line has no direction. Must return
        // None so the gradient shader falls back to solid colour.
        assert!(project_p3((10.0, 20.0), (50.0, 50.0), (10.0, 20.0)).is_none());
        // Near-coincident (within epsilon) also rejected.
        assert!(
            project_p3((10.0, 20.0), (50.0, 50.0), (10.0 + 1e-4, 20.0 + 1e-4)).is_none()
        );
    }

    #[test]
    fn project_p3_p1_already_on_perpendicular_returns_p1() {
        // If P1 is already on the perpendicular through P0 (i.e. its
        // projection onto P0→P2 is at P0 itself), P3 should equal P1
        // exactly.
        let p0 = (0.0, 0.0);
        let p2 = (10.0, 0.0);
        let p1 = (0.0, 5.0); // perpendicular to x-axis at origin
        let (p3x, p3y) = project_p3(p0, p1, p2).unwrap();
        assert!((p3x - p1.0).abs() < 1e-6);
        assert!((p3y - p1.1).abs() < 1e-6);
    }

    #[test]
    fn glyf_bbox_reads_signed_bbox() {
        // numContours=1 (0x0001), x_min=-100, y_min=-200, x_max=300, y_max=700.
        let bytes = [
            0x00, 0x01, // numContours
            0xFF, 0x9C, // -100
            0xFF, 0x38, // -200
            0x01, 0x2C, // 300
            0x02, 0xBC, // 700
        ];
        assert_eq!(glyf_bbox(&bytes), Some((-100, -200, 300, 700)));
    }

    #[test]
    fn glyf_bbox_rejects_short_input() {
        assert_eq!(glyf_bbox(&[]), None);
        assert_eq!(glyf_bbox(&[0; 9]), None);
    }

    /// Build a minimal COLR v1 header + BaseGlyphList payload.
    /// `base_glyph_ids` becomes the list of GlyphIDs written as
    /// BaseGlyphPaintRecord entries, in order.
    fn build_colr_v1(base_glyph_ids: &[u16]) -> Vec<u8> {
        let mut out = Vec::new();
        // Header: version=1, num_v0=0, v0_off=0, layer_off=0, num_layers=0.
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        // base_glyph_list_offset — points right after the v1 header
        // (32 bytes total: 14 v0 + 4 (v1_base) + 4 (v1_layer) +
        // 4 (v1_clip) + 4 (varindex) + 4 (variationstore)).
        let list_off: u32 = 34;
        out.extend_from_slice(&list_off.to_be_bytes());
        // layer_list_offset, clip_list_offset, var_index_map_offset,
        // item_variation_store_offset — all 0, unused.
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        assert_eq!(out.len(), list_off as usize);
        // BaseGlyphList: num_records: u32, then (u16 gid, u32 paint_off).
        out.extend_from_slice(&(base_glyph_ids.len() as u32).to_be_bytes());
        for &gid in base_glyph_ids {
            out.extend_from_slice(&gid.to_be_bytes());
            out.extend_from_slice(&0u32.to_be_bytes());
        }
        out
    }

    #[test]
    fn first_base_glyph_id_picks_first_non_empty() {
        // GID 0 is empty (.notdef), GID 1 has outline bytes — the
        // subsetted-Nabla case. Must return 1, not 0.
        let colr = build_colr_v1(&[0, 1]);
        let glyphs: Vec<Vec<u8>> = vec![
            vec![],              // GID 0: empty
            vec![0xA, 0xB, 0xC], // GID 1: has bytes
        ];
        assert_eq!(first_base_glyph_id(&colr, &glyphs), Some(1));
    }

    #[test]
    fn first_base_glyph_id_honours_record_order() {
        // All GIDs non-empty → returns the first one.
        let colr = build_colr_v1(&[3, 1, 7]);
        let glyphs: Vec<Vec<u8>> = vec![vec![1]; 10]; // GID 0..9 all non-empty
        assert_eq!(first_base_glyph_id(&colr, &glyphs), Some(3));
    }

    #[test]
    fn first_base_glyph_id_falls_back_to_first_record_when_all_empty() {
        // Every record points at an empty outline (pathological case
        // where the subsetter kept placeholders only). Return the
        // first record's GID so the caller's bbox read bails cleanly
        // rather than panicking on an `expect_some`.
        let colr = build_colr_v1(&[5, 10]);
        let glyphs: Vec<Vec<u8>> = vec![vec![]; 20];
        assert_eq!(first_base_glyph_id(&colr, &glyphs), Some(5));
    }

    #[test]
    fn first_base_glyph_id_handles_empty_colr_table() {
        // < 8 bytes: nothing to parse.
        assert_eq!(first_base_glyph_id(&[], &[]), None);
        assert_eq!(first_base_glyph_id(&[0, 0, 0, 0, 0, 0, 0, 0], &[]), None);
    }

    #[test]
    fn composite_mode_to_blend_covers_every_variant() {
        // Every CompositeMode variant from ttf-parser's COLR spec
        // (§Format 32 Paint​Composite) must map to some tiny-skia
        // BlendMode. Exhaustive enum match catches a missing arm at
        // compile time, but this test also guarantees the common
        // `SourceOver` → `SourceOver` pairing — the one the layer
        // stack falls back on when nothing special is in play.
        use CompositeMode::*;
        assert_eq!(composite_mode_to_blend(SourceOver), BlendMode::SourceOver);
        assert_eq!(composite_mode_to_blend(Clear), BlendMode::Clear);
        assert_eq!(composite_mode_to_blend(Xor), BlendMode::Xor);
        assert_eq!(composite_mode_to_blend(Plus), BlendMode::Plus);
        assert_eq!(composite_mode_to_blend(Multiply), BlendMode::Multiply);
        assert_eq!(composite_mode_to_blend(Luminosity), BlendMode::Luminosity);
    }

    #[test]
    fn extend_to_spread_maps_all_three_modes() {
        assert_eq!(extend_to_spread(GradientExtend::Pad), SpreadMode::Pad);
        assert_eq!(extend_to_spread(GradientExtend::Repeat), SpreadMode::Repeat);
        assert_eq!(
            extend_to_spread(GradientExtend::Reflect),
            SpreadMode::Reflect
        );
    }

    #[test]
    fn intersect_masks_multiplies_alpha_channels() {
        let mut dst = Mask::new(2, 2).unwrap();
        let mut src = Mask::new(2, 2).unwrap();
        // Manually set the alpha bytes: dst = 255,128,64,0; src = 128,255,128,255.
        dst.data_mut().copy_from_slice(&[255, 128, 64, 0]);
        src.data_mut().copy_from_slice(&[128, 255, 128, 255]);

        intersect_masks(&mut dst, &src);

        // (255 * 128) / 255 = 128
        // (128 * 255) / 255 = 128
        // (64  * 128) / 255 = 32  (integer division)
        // (0   * 255) / 255 = 0
        assert_eq!(dst.data(), &[128, 128, 32, 0]);
    }

    #[test]
    fn intersect_masks_ignores_mismatched_sizes() {
        // Precondition: we only call intersect_masks on masks with
        // the same dimensions. If that ever fails, we leave dst as-is
        // rather than panicking.
        let mut dst = Mask::new(2, 2).unwrap();
        dst.data_mut().copy_from_slice(&[0x55; 4]);
        let src = Mask::new(4, 2).unwrap();
        intersect_masks(&mut dst, &src);
        assert_eq!(dst.data(), &[0x55; 4]);
    }
}
