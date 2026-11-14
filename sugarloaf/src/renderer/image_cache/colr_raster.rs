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
use crate::font::glyph_registry::{GlyphAlignH, GlyphAlignV, GlyphLayout, GlyphSize};

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
    pub is_color: bool,
}

/// Rasterise a registered Glyph Protocol payload. Dispatches the
/// monochrome `glyf` path through tiny-skia's anti-aliased fill (A8
/// output) and the colour `colrv0`/`colrv1` paths through the COLR
/// painter graph (RGBA8 output). Returns `None` on malformed payload
/// or degenerate sizing.
///
/// `cell_width_px` is the declared render span (1 or 2 cell widths),
/// and the bitmap is always exactly that span by `cell_height_px`.
/// `baseline_px` is the text baseline measured from the top of the
/// cell, used by `align=_,baseline`. See [`SpanPlacement`].
pub fn rasterize_payload(
    payload: &crate::font::glyph_registry::StoredPayload,
    upm: u16,
    cell_width_px: u16,
    cell_height_px: u16,
    foreground_rgba: [u8; 4],
    layout: &GlyphLayout,
    baseline_px: f32,
) -> Option<RasterizedPayload> {
    use crate::font::glyph_registry::StoredPayload;
    match payload {
        StoredPayload::Glyf { glyf } => rasterize_mono(
            glyf,
            upm,
            cell_width_px,
            cell_height_px,
            layout,
            baseline_px,
        ),
        StoredPayload::ColrV0 { glyphs, colr, cpal }
        | StoredPayload::ColrV1 { glyphs, colr, cpal } => rasterize(
            glyphs,
            colr,
            cpal,
            upm,
            cell_width_px,
            cell_height_px,
            foreground_rgba,
            layout,
            baseline_px,
        ),
    }
}

/// An outline placed by the §8.5 model: `pad`, then `size`, then
/// `align`, all on the authored extent. A design point `(x, y)` lands
/// on pixel `(tx + x * sx, ty - y * sy)` of a bitmap exactly the size of
/// the render span, which is what clips the glyph to its span (§8.5.7).
struct SpanPlacement {
    sx: f32,
    sy: f32,
    tx: f32,
    ty: f32,
}

impl SpanPlacement {
    /// `bounds` yields the outline's `[x_min, y_min, x_max, y_max]`; it
    /// is only asked for when the extent isn't declared.
    fn new(
        layout: &GlyphLayout,
        upm: u16,
        span_w_px: u16,
        cell_h_px: u16,
        baseline_px: f32,
        bounds: impl FnOnce() -> Option<[f32; 4]>,
    ) -> Option<Self> {
        // §8.5.1: the declared extent, or the outline's bounds without one.
        let declared =
            layout.aw.is_some() || layout.lh.is_some() || layout.ymin.is_some();
        let (x0, aw, y0, lh) = if declared {
            (
                0.0,
                layout.aw.unwrap_or(upm) as f32,
                layout.ymin.unwrap_or(0) as f32,
                layout.lh.unwrap_or(upm) as f32,
            )
        } else {
            let [x_min, y_min, x_max, y_max] = bounds()?;
            (x_min, x_max - x_min, y_min, y_max - y_min)
        };
        if upm == 0 || aw <= 0.0 || lh <= 0.0 || span_w_px == 0 || cell_h_px == 0 {
            return None;
        }

        // §8.5.2: the effective span.
        let [pad_top, pad_right, pad_bottom, pad_left] = layout.pad;
        let (span_w, span_h) = (span_w_px as f32, cell_h_px as f32);
        let (inset_x, inset_y) = (span_w * pad_left, span_h * pad_top);
        let w = span_w * (1.0 - pad_left - pad_right);
        let h = span_h * (1.0 - pad_top - pad_bottom);

        // §8.5.3: scale factors.
        let (fit_w, fit_h) = (w / aw, h / lh);
        // Without a declared extent the em keeps a set in proportion.
        let contain = if declared {
            fit_w.min(fit_h)
        } else {
            fit_w.min(fit_h).min(w.min(h) / upm as f32)
        };
        let (sx, sy) = match layout.size {
            GlyphSize::Height => (fit_h, fit_h),
            GlyphSize::Advance => (fit_w, fit_w),
            GlyphSize::Contain => (contain, contain),
            GlyphSize::Cover => (fit_w.max(fit_h), fit_w.max(fit_h)),
            GlyphSize::Stretch => (fit_w, fit_h),
        };

        // §8.5.4: where the scaled extent sits in the span.
        let tx = inset_x - x0 * sx
            + match layout.align_h {
                GlyphAlignH::Start => 0.0,
                GlyphAlignH::Center => (w - aw * sx) / 2.0,
                GlyphAlignH::End => w - aw * sx,
            };
        // `stretch` fills the span, so `align` has no effect on it.
        let bottom = inset_y + h + y0 * sy;
        let ty = match layout.align_v {
            _ if layout.size == GlyphSize::Stretch => bottom,
            GlyphAlignV::Start => bottom,
            GlyphAlignV::Center => inset_y + h / 2.0 + (y0 + lh / 2.0) * sy,
            GlyphAlignV::End => inset_y + (y0 + lh) * sy,
            GlyphAlignV::Baseline => baseline_px,
        };

        Some(Self { sx, sy, tx, ty })
    }
}

/// Walk a `glyf` simple-glyph outline and rasterise it as an A8 alpha
/// mask sized to fit `pixel_size`. The atlas-bound caller uploads
/// the bytes straight into the grayscale atlas, same shape as the
/// swash/CT mono path produces.
fn rasterize_mono(
    glyf: &[u8],
    upm: u16,
    cell_width_px: u16,
    cell_height_px: u16,
    layout: &GlyphLayout,
    baseline_px: f32,
) -> Option<RasterizedPayload> {
    let outline = glyf_decode::decode(glyf).ok()?;

    let span = SpanPlacement::new(
        layout,
        upm,
        cell_width_px,
        cell_height_px,
        baseline_px,
        || {
            Some([
                outline.x_min as f32,
                outline.y_min as f32,
                outline.x_max as f32,
                outline.y_max as f32,
            ])
        },
    )?;
    let (pix_w, pix_h) = (cell_width_px as u32, cell_height_px as u32);
    // `Outline::walk` (below) is Y-down from the bbox top, hence the
    // `y_max` shift.
    let ctm = Transform::from_row(
        span.sx,
        0.0,
        0.0,
        span.sy,
        span.tx,
        span.ty - outline.y_max as f32 * span.sy,
    );

    // `glyf_decode::Outline::walk` flips Y so its output is Y-down
    // with origin at the top of the bbox (y=0 → top, y increases
    // downward to `y_max - y_min`). That matches tiny-skia's pixmap
    // convention exactly, so we feed walk's coords straight in
    // without further flipping. The COLR rasteriser un-flips because
    // its painter expects Y-up design units; this monochrome path
    // skips the painter and goes pixmap-direct.
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
        is_color: false,
    })
}

/// Parse a colour container's `COLR` table against its `CPAL`.
///
/// A container may ship no CPAL (legal for v1 paints that only use
/// direct or foreground colours), but `cpal::Table::parse` rejects
/// anything without at least one palette. Feed it the smallest valid
/// table: one palette holding no colours. Foreground paints never
/// consult it, and a real palette lookup still fails.
fn parse_colr<'a>(
    colr_bytes: &'a [u8],
    cpal_bytes: &'a [u8],
) -> Option<ttf_parser::colr::Table<'a>> {
    const EMPTY_CPAL: [u8; 14] = [
        0x00, 0x00, // version
        0x00, 0x00, // numPaletteEntries
        0x00, 0x01, // numPalettes
        0x00, 0x00, // numColorRecords
        0x00, 0x00, 0x00, 0x0E, // colorRecordsArrayOffset
        0x00, 0x00, // colorRecordIndices[0]
    ];
    let cpal_source: &[u8] = if cpal_bytes.is_empty() {
        &EMPTY_CPAL
    } else {
        cpal_bytes
    };
    let cpal = ttf_parser::cpal::Table::parse(cpal_source)?;
    ttf_parser::colr::Table::parse(cpal, colr_bytes)
}

/// Records every colour a paint graph asks for, drawing nothing.
#[derive(Default)]
struct PaintedColors(Vec<RgbaColor>);

impl<'a> Painter<'a> for PaintedColors {
    fn outline_glyph(&mut self, _: GlyphId) {}
    fn paint(&mut self, paint: Paint<'a>) {
        match paint {
            Paint::Solid(color) => self.0.push(color),
            Paint::LinearGradient(g) => self.0.extend(g.stops(0, &[]).map(|s| s.color)),
            Paint::RadialGradient(g) => self.0.extend(g.stops(0, &[]).map(|s| s.color)),
            Paint::SweepGradient(g) => self.0.extend(g.stops(0, &[]).map(|s| s.color)),
        }
    }
    fn push_clip(&mut self) {}
    fn push_clip_box(&mut self, _: ClipBox) {}
    fn pop_clip(&mut self) {}
    fn push_layer(&mut self, _: CompositeMode) {}
    fn pop_layer(&mut self) {}
    fn push_transform(&mut self, _: TtfTransform) {}
    fn pop_transform(&mut self) {}
}

/// Whether a payload's pixels depend on the foreground colour. Walks the
/// paint graph under two different foregrounds: a graph that references
/// the foreground asks for different colours, whatever the cell size.
/// Alpha masks take their colour at draw time, so they don't.
pub fn payload_uses_foreground(
    payload: &crate::font::glyph_registry::StoredPayload,
) -> bool {
    use crate::font::glyph_registry::StoredPayload;
    let (glyphs, colr, cpal) = match payload {
        StoredPayload::Glyf { .. } => return false,
        StoredPayload::ColrV0 { glyphs, colr, cpal }
        | StoredPayload::ColrV1 { glyphs, colr, cpal } => (glyphs, colr, cpal),
    };
    let (Some(table), Some(base_gid)) =
        (parse_colr(colr, cpal), first_base_glyph_id(colr, glyphs))
    else {
        return false;
    };
    let colors_under = |foreground| {
        let mut painted = PaintedColors::default();
        table
            .paint(GlyphId(base_gid), 0, &mut painted, &[], foreground)
            .map(|()| painted.0)
    };
    colors_under(RgbaColor::new(0, 0, 0, 255))
        != colors_under(RgbaColor::new(255, 255, 255, 255))
}

/// Rasterise a COLR glyph to RGBA. Returns `None` when COLR/CPAL is
/// malformed, when the base-glyph outline is empty, or when tiny-skia
/// rejects a degenerate configuration (e.g. zero pixmap size).
#[allow(clippy::too_many_arguments)]
pub(super) fn rasterize(
    glyphs: &[Vec<u8>],
    colr_bytes: &[u8],
    cpal_bytes: &[u8],
    upm: u16,
    cell_width_px: u16,
    cell_height_px: u16,
    foreground: [u8; 4],
    layout: &GlyphLayout,
    baseline_px: f32,
) -> Option<RasterizedPayload> {
    // Cheap guard before the COLR parse.
    if upm == 0 || cell_width_px == 0 || cell_height_px == 0 {
        return None;
    }

    let colr = parse_colr(colr_bytes, cpal_bytes)?;
    let base_gid = first_base_glyph_id(colr_bytes, glyphs)?;

    // The outline's bounds, used only when the extent isn't declared.
    // Prefer the COLR ClipBox: emoji fonts (Noto Color Emoji, etc.) have
    // empty wrapper base glyphs that reference layers via the paint
    // graph. Fall back to the base glyph's own points for fonts like
    // Nabla that carry geometry on the base glyph.
    let bounds = || match colr.clip_box(GlyphId(base_gid), &[]) {
        Some(cb) => Some([
            cb.x_min.floor(),
            cb.y_min.floor(),
            cb.x_max.ceil(),
            cb.y_max.ceil(),
        ]),
        None => {
            let glyph = glyphs.get(base_gid as usize)?;
            let (x_min, y_min, x_max, y_max) = glyf_decode::bounds(glyph).ok()?;
            Some([x_min as f32, y_min as f32, x_max as f32, y_max as f32])
        }
    };
    let span = SpanPlacement::new(
        layout,
        upm,
        cell_width_px,
        cell_height_px,
        baseline_px,
        bounds,
    )?;
    let (pix_w, pix_h) = (cell_width_px as u32, cell_height_px as u32);

    // Font units (y-up, origin at baseline) → pixmap pixels (y-down,
    // origin at top-left). The identity-sized glyph outline in the
    // painter gets this transform applied before drawing.
    // Matrix [sx, ky, kx, sy, tx, ty] applies (x,y) as
    //   (sx*x + kx*y + tx, ky*x + sy*y + ty).
    let base_ctm = Transform::from_row(span.sx, 0.0, 0.0, -span.sy, span.tx, span.ty);

    let base_pixmap = Pixmap::new(pix_w, pix_h)?;

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

    /// Build a `glyf` simple-glyph inked only across the top
    /// `strip_height` rows of an `em_top` square (design Y from
    /// `em_top - strip_height` to `em_top`).
    fn glyf_top_strip(em_top: i16, strip_height: i16) -> Vec<u8> {
        // glyf simple-glyph layout per OpenType:
        //   i16 numberOfContours
        //   i16 xMin, yMin, xMax, yMax
        //   u16 endPtsOfContours[numContours]
        //   u16 instructionLength
        //   u8  flags[numPoints]
        //   coords (deltas, big-endian i16 when not using shorts)
        let strip_bottom = em_top - strip_height;
        let mut v = Vec::new();
        v.extend_from_slice(&1i16.to_be_bytes()); // numberOfContours
        v.extend_from_slice(&0i16.to_be_bytes()); // xMin
        v.extend_from_slice(&strip_bottom.to_be_bytes()); // yMin
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

    #[test]
    fn rasterize_mono_top_pixels_filled_bottom_pixels_empty() {
        // Outline occupies only the top 25% of the em — after raster,
        // the top rows of the bitmap should be inked and the bottom
        // rows should be transparent. Catches the Y-flip bug we hit
        // earlier (where the strip rendered at the bottom instead).
        let upm = 100i16;
        let bytes = glyf_top_strip(upm, upm / 4);
        let em = GlyphLayout {
            aw: Some(upm as u16),
            lh: Some(upm as u16),
            ..Default::default()
        };
        let r = rasterize_mono(&bytes, upm as u16, upm as u16, upm as u16, &em, 0.0)
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
    fn rasterize_mono_contains_outline_larger_than_em() {
        // The outline bbox (200 design units) is twice the declared em
        // (upm=100). Naive em-proportional scaling (min(cell)/upm) would
        // render it at 2× the cell and bleed into the neighbour; the
        // contain clamp keeps the pixmap within the cell on both axes.
        let bytes = glyf_top_strip(200, 50);
        let r = rasterize_mono(&bytes, 100, 20, 40, &GlyphLayout::default(), 0.0)
            .expect("rasterizes");
        assert_eq!((r.width, r.height), (20, 40));
        // 20px wide at 0.1 px/unit: the whole span, not twice it.
        assert_eq!(ink(&r.data, r.width).0, 0);
        assert_eq!(ink(&r.data, r.width).1, 19);
    }

    #[test]
    fn rasterize_mono_contains_hostile_upm() {
        // upm=1 is wire-legal (only upm=0 is rejected by the parser).
        // Under naive em-scaling, an outline spanning ~30000 design
        // units at upm=1 would scale by 16/1 → a ~480000px-wide pixmap
        // (multi-gigabyte allocation / DoS). The contain clamp bounds
        // the raster to the cell regardless of upm or outline span, so
        // the glyph rasterises safely instead.
        let bytes = glyf_top_strip(30000, 7500);
        let r = rasterize_mono(&bytes, 1, 16, 32, &GlyphLayout::default(), 0.0)
            .expect("hostile upm should be contained, not rejected");
        assert_eq!((r.width, r.height), (16, 32));
        assert_eq!(ink(&r.data, r.width).1, 15);
    }

    /// A filled rectangle as a `glyf` simple glyph.
    fn glyf_box(x_min: i16, y_min: i16, x_max: i16, y_max: i16) -> Vec<u8> {
        let mut v = Vec::new();
        for field in [1, x_min, y_min, x_max, y_max, 3, 0] {
            v.extend_from_slice(&field.to_be_bytes());
        }
        v.extend_from_slice(&[0x01; 4]);
        for delta in [x_min, x_max - x_min, 0, x_min - x_max] {
            v.extend_from_slice(&delta.to_be_bytes());
        }
        for delta in [y_min, 0, y_max - y_min, 0] {
            v.extend_from_slice(&delta.to_be_bytes());
        }
        v
    }

    /// Inclusive `(first_col, last_col, first_row, last_row)` of the
    /// mostly-covered pixels.
    fn ink(alpha: &[u8], width: u16) -> (usize, usize, usize, usize) {
        let width = width as usize;
        let inked = || alpha.iter().enumerate().filter(|(_, a)| **a >= 128);
        let cols = || inked().map(|(i, _)| i % width);
        let rows = || inked().map(|(i, _)| i / width);
        (
            cols().min().expect("some ink"),
            cols().max().unwrap(),
            rows().min().unwrap(),
            rows().max().unwrap(),
        )
    }

    fn place(glyf: &[u8], upm: u16, layout: GlyphLayout) -> Option<RasterizedPayload> {
        rasterize_mono(glyf, upm, 10, 20, &layout, 15.0)
    }

    #[test]
    fn layout_advance_fits_authored_advance_to_cell_on_baseline() {
        // Departure Mono's shape: em 550, advance 350, cap 400.
        let glyph = glyf_box(50, 0, 300, 400);
        let layout = GlyphLayout {
            aw: Some(350),
            size: GlyphSize::Advance,
            align_v: GlyphAlignV::Baseline,
            ..Default::default()
        };
        let r = place(&glyph, 550, layout).unwrap();
        assert_eq!((r.width, r.height), (10, 20));
        // x: 50..300 at 10/350 px per unit; y: up from the baseline.
        assert_eq!(ink(&r.data, r.width), (1, 8, 4, 14));
        // `aw` alone sets the scale, so `upm` doesn't matter.
        assert_eq!(place(&glyph, 350, layout).unwrap().data, r.data);
    }

    #[test]
    fn layout_height_scales_by_line_height() {
        let layout = GlyphLayout {
            aw: Some(50),
            lh: Some(100),
            size: GlyphSize::Height,
            ..Default::default()
        };
        let r = place(&glyf_box(0, 0, 50, 100), 1000, layout).unwrap();
        assert_eq!(ink(&r.data, r.width), (0, 9, 0, 19));
    }

    #[test]
    fn layout_contain_stays_inside_the_span() {
        let layout = GlyphLayout {
            size: GlyphSize::Contain,
            ..Default::default()
        };
        let r = place(&glyf_box(0, 0, 100, 100), 100, layout).unwrap();
        assert_eq!(r.width, 10);
        // 10px square, centred in the 20px-tall cell.
        assert_eq!(ink(&r.data, r.width), (0, 9, 5, 14));
    }

    #[test]
    fn layout_cover_clips_to_the_span() {
        let layout = GlyphLayout {
            size: GlyphSize::Cover,
            ..Default::default()
        };
        let r = place(&glyf_box(0, 0, 100, 100), 100, layout).unwrap();
        assert_eq!((r.width, r.height), (10, 20));
        assert_eq!(ink(&r.data, r.width), (0, 9, 0, 19));
    }

    #[test]
    fn layout_clips_vertical_overflow_to_the_cell() {
        // One px per unit, 40 tall in a 20px cell, centred on lh=40.
        let tall = GlyphLayout {
            aw: Some(10),
            lh: Some(40),
            size: GlyphSize::Advance,
            ..Default::default()
        };
        let r = place(&glyf_box(0, 0, 10, 40), 10, tall).unwrap();
        assert_eq!(r.height, 20);
        assert_eq!(ink(&r.data, r.width), (0, 9, 0, 19));
    }

    #[test]
    fn layout_stretch_fills_the_span_whatever_the_alignment() {
        let layout = GlyphLayout {
            size: GlyphSize::Stretch,
            align_h: GlyphAlignH::End,
            align_v: GlyphAlignV::Start,
            ..Default::default()
        };
        let r = place(&glyf_box(0, 0, 100, 100), 100, layout).unwrap();
        assert_eq!(ink(&r.data, r.width), (0, 9, 0, 19));
    }

    #[test]
    fn layout_pad_insets_the_span() {
        let layout = GlyphLayout {
            aw: Some(50),
            lh: Some(50),
            pad: [0.75, 0.5, 0.0, 0.0],
            ..Default::default()
        };
        // The span shrinks to the bottom-left 5x5 pixels.
        let r = place(&glyf_box(0, 0, 50, 50), 100, layout).unwrap();
        assert_eq!(ink(&r.data, r.width), (0, 4, 15, 19));
    }

    #[test]
    fn layout_alignment_anchors_the_authored_box() {
        let vertical = |align_v| {
            let layout = GlyphLayout {
                size: GlyphSize::Contain,
                align_v,
                ..Default::default()
            };
            let r = place(&glyf_box(0, 0, 100, 100), 100, layout).unwrap();
            ink(&r.data, r.width)
        };
        // A 10px square, pinned to the bottom or top of the 20px cell.
        assert_eq!(vertical(GlyphAlignV::Start), (0, 9, 10, 19));
        assert_eq!(vertical(GlyphAlignV::End), (0, 9, 0, 9));

        let horizontal = |align_h| {
            let layout = GlyphLayout {
                aw: Some(20),
                lh: Some(100),
                align_h,
                ..Default::default()
            };
            let r = place(&glyf_box(0, 0, 20, 100), 100, layout).unwrap();
            let (first, last, _, _) = ink(&r.data, r.width);
            (first, last)
        };
        // A 4px-wide box in the 10px span.
        assert_eq!(horizontal(GlyphAlignH::Start), (0, 3));
        assert_eq!(horizontal(GlyphAlignH::Center), (3, 6));
        assert_eq!(horizontal(GlyphAlignH::End), (6, 9));
    }

    #[test]
    fn layout_default_centres_an_outline_below_the_baseline() {
        // y from -150 to 850 in a 1000-unit em: at the em cap of 0.01
        // px/unit it is 10px tall and centred in the 20px cell.
        let glyph = glyf_box(250, -150, 750, 850);
        let r = place(&glyph, 1000, GlyphLayout::default()).unwrap();
        let (_, _, first_row, last_row) = ink(&r.data, r.width);
        assert_eq!((first_row, last_row), (5, 14));
    }

    #[test]
    fn layout_undeclared_extent_ignores_a_lying_header() {
        let mut lying = glyf_box(0, 0, 100, 100);
        // Claim the outline is far smaller than it is.
        lying[2..10].copy_from_slice(&[0, 40, 0, 40, 0, 41, 0, 41]);
        let honest = place(&glyf_box(0, 0, 100, 100), 100, GlyphLayout::default());
        let lied = place(&lying, 100, GlyphLayout::default());
        assert_eq!(lied.unwrap().data, honest.unwrap().data);
    }

    #[test]
    fn layout_undeclared_extent_keeps_a_set_in_proportion() {
        // A small dot stays small: 0.01 px/unit, not grown to the span.
        let dot = place(&glyf_box(400, 0, 600, 200), 1000, GlyphLayout::default());
        let dot = dot.unwrap();
        assert_eq!(ink(&dot.data, dot.width), (4, 5, 9, 10));
    }

    #[test]
    fn layout_default_contains_an_undeclared_outline_in_its_span() {
        // Twice as wide as the span and off-centre in its own em.
        let r = place(&glyf_box(100, 0, 300, 120), 200, GlyphLayout::default()).unwrap();
        assert_eq!(r.width, 10);
        // 200x120 fits at 0.05 px/unit: 10x6, centred in the 20px cell.
        assert_eq!(ink(&r.data, r.width), (0, 9, 7, 12));
    }

    #[test]
    fn layout_ymin_positions_the_declared_extent() {
        let extent = GlyphLayout {
            aw: Some(10),
            lh: Some(20),
            ymin: Some(-5),
            ..Default::default()
        };
        // 1 px/unit: an outline filling the extent fills the span.
        let r = place(&glyf_box(0, -5, 10, 15), 1000, extent).unwrap();
        assert_eq!(ink(&r.data, r.width), (0, 9, 0, 19));

        // Without `ymin` the same outline is shoved down and clipped.
        let unshifted = GlyphLayout {
            ymin: None,
            ..extent
        };
        let r = place(&glyf_box(0, -5, 10, 15), 1000, unshifted).unwrap();
        assert_eq!(ink(&r.data, r.width), (0, 9, 5, 19));
    }

    #[test]
    fn layout_declared_extent_ignores_each_outlines_own_bounds() {
        let extent = GlyphLayout {
            aw: Some(10),
            lh: Some(20),
            ymin: Some(-5),
            ..Default::default()
        };
        // A small glyph stays where it was drawn inside the extent.
        let r = place(&glyf_box(2, 0, 4, 5), 1000, extent).unwrap();
        assert_eq!(ink(&r.data, r.width), (2, 3, 10, 14));
    }

    #[test]
    fn layout_bounds_extreme_overflow_to_the_span() {
        let layout = GlyphLayout {
            aw: Some(1),
            size: GlyphSize::Advance,
            ..Default::default()
        };
        let r = place(&glyf_box(0, 0, 32000, 10), 1000, layout).unwrap();
        assert_eq!((r.width, r.height), (10, 20));
    }

    /// COLR v1 painting glyph 0 in the foreground colour.
    fn colr_v1_foreground() -> Vec<u8> {
        let mut colr = build_colr_v1(&[0]);
        // Point the base record (at 40) past the list, at PaintGlyph.
        colr[40..44].copy_from_slice(&10u32.to_be_bytes());
        colr.extend_from_slice(&[10, 0, 0, 6, 0, 0]); // PaintGlyph(gid 0)
        colr.extend_from_slice(&[2, 0xFF, 0xFF, 0x40, 0x00]); // PaintSolid(fg)
        colr
    }

    fn rasterize_foreground(layout: &GlyphLayout) -> Option<RasterizedPayload> {
        rasterize(
            // Whole pixels at 35 units/px, so edges carry no coverage.
            &[glyf_box(70, 0, 280, 350)],
            &colr_v1_foreground(),
            &[],
            550,
            10,
            20,
            [255, 255, 255, 255],
            layout,
            15.0,
        )
    }

    fn alpha_of(rgba: &[u8]) -> Vec<u8> {
        rgba.chunks(4).map(|pixel| pixel[3]).collect()
    }

    #[test]
    fn colr_declared_extent_needs_no_base_glyph_bounds() {
        // An empty wrapper base glyph painting layer glyph 1, no ClipList.
        let glyphs = [Vec::new(), glyf_box(70, 0, 280, 350)];
        let mut colr = colr_v1_foreground();
        colr[49] = 1; // PaintGlyph's glyph id
        let draw = |layout: &GlyphLayout| {
            rasterize(
                &glyphs,
                &colr,
                &[],
                550,
                10,
                20,
                [255, 255, 255, 255],
                layout,
                15.0,
            )
        };
        let declared = GlyphLayout {
            aw: Some(350),
            size: GlyphSize::Advance,
            align_v: GlyphAlignV::Baseline,
            ..Default::default()
        };
        let r = draw(&declared).expect("a declared extent needs no bounds");
        assert_eq!(ink(&alpha_of(&r.data), r.width), (2, 7, 5, 14));
        // Without a declared extent there is nothing to place it by.
        assert!(draw(&GlyphLayout::default()).is_none());
    }

    #[test]
    fn colr_measures_a_hinted_base_glyph_it_never_paints() {
        // A hinted monochrome base, unhinted layer, and no ClipList.
        let mut hinted = glyf_box(70, 0, 280, 350);
        hinted[12..14].copy_from_slice(&1u16.to_be_bytes());
        hinted.insert(14, 0x00);
        let glyphs = [hinted, glyf_box(70, 0, 280, 350)];
        let mut colr = colr_v1_foreground();
        colr[49] = 1; // PaintGlyph's glyph id
        let r = rasterize(
            &glyphs,
            &colr,
            &[],
            550,
            10,
            20,
            [255, 255, 255, 255],
            &GlyphLayout::default(),
            15.0,
        )
        .expect("the base glyph is only measured");
        assert!(alpha_of(&r.data).iter().any(|alpha| *alpha >= 128));
    }

    #[test]
    fn foreground_use_comes_from_the_paint_graph() {
        use crate::font::glyph_registry::StoredPayload;

        let glyphs = vec![glyf_box(70, 0, 280, 350)];
        let foreground = StoredPayload::ColrV1 {
            glyphs: glyphs.clone(),
            colr: colr_v1_foreground(),
            cpal: Vec::new(),
        };
        assert!(payload_uses_foreground(&foreground));

        // The same graph painting palette entry 0 instead.
        let mut palette_colr = colr_v1_foreground();
        let solid = palette_colr.len() - 4;
        palette_colr[solid..solid + 2].copy_from_slice(&[0, 0]);
        let palette = StoredPayload::ColrV1 {
            glyphs,
            colr: palette_colr,
            cpal: vec![0, 0, 0, 1, 0, 1, 0, 1, 0, 0, 0, 14, 0, 0, 0, 0, 0, 255],
        };
        assert!(!payload_uses_foreground(&palette));

        assert!(!payload_uses_foreground(&StoredPayload::glyf(vec![1])));
    }

    #[test]
    fn colr_without_cpal_renders_foreground_paints() {
        let r = rasterize_foreground(&GlyphLayout::default())
            .expect("spec §8.7 permits cpal_len = 0");
        assert!(alpha_of(&r.data).iter().any(|alpha| *alpha >= 128));
    }

    #[test]
    fn layout_applies_to_colr_with_the_same_orientation() {
        let layout = GlyphLayout {
            aw: Some(350),
            size: GlyphSize::Advance,
            align_v: GlyphAlignV::Baseline,
            ..Default::default()
        };
        let r = rasterize_foreground(&layout).expect("colour glyph rasterizes");
        assert_eq!(ink(&alpha_of(&r.data), r.width), (2, 7, 5, 14));
    }

    #[test]
    fn rasterize_mono_rejects_zero_cell_or_upm() {
        let bytes = glyf_top_strip(100, 25);
        assert!(
            rasterize_mono(&bytes, 100, 0, 16, &GlyphLayout::default(), 0.0).is_none()
        );
        assert!(
            rasterize_mono(&bytes, 100, 16, 0, &GlyphLayout::default(), 0.0).is_none()
        );
        assert!(
            rasterize_mono(&bytes, 0, 16, 16, &GlyphLayout::default(), 0.0).is_none()
        );
    }

    #[test]
    fn rasterize_mono_fits_em_square_into_cell_box() {
        // A full-em-wide strip spans the narrow cell axis exactly: the
        // em square is contained in the cell, not font-size-wide.
        let upm = 100i16;
        let bytes = glyf_top_strip(upm, upm / 4);
        let (cell_w, cell_h) = (10u16, 20u16);
        let r = rasterize_mono(
            &bytes,
            upm as u16,
            cell_w,
            cell_h,
            &GlyphLayout::default(),
            0.0,
        )
        .expect("rasterize succeeds for valid simple glyph");
        assert_eq!(r.width, cell_w);
        assert_eq!(ink(&r.data, r.width).1, cell_w as usize - 1);
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
