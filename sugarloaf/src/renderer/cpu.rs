// Copyright (c) 2023-present, Raphael Amorim.
//
// CPU rasterization pipeline.
//
// Writes directly into softbuffer's `&mut [u32]` (0x00RRGGBB) — no
// intermediate pixmap, no pixel format conversion at present time.
//
// v1 limitations: monochrome glyphs only (color-atlas glyphs / images
// not implemented), no per-corner radii / borders / advanced underlines.

use crate::context::cpu::CpuContext;
use crate::renderer::compositor::Vertex;
use crate::renderer::image_cache::ImageCache;
use crate::renderer::Renderer;
use rustc_hash::FxHashMap;
use std::hash::Hasher;
use wide::{u32x4, u32x8};

#[derive(Default)]
pub struct CpuCache {
    glyphs: FxHashMap<GlyphKey, CachedGlyph>,
    last_frame_hash: u64,
    has_last: bool,
}

impl CpuCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.has_last = false;
    }
}

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
struct GlyphKey {
    atlas_x: u16,
    atlas_y: u16,
    w: u16,
    h: u16,
    color: u32,
}

struct CachedGlyph {
    /// Length = w * h. Premultiplied (a<<24)|(r<<16)|(g<<8)|b.
    pixels: Vec<u32>,
    w: u16,
    h: u16,
}

#[inline(always)]
fn pack_premul(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((a as u32) << 24) | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[inline(always)]
fn pack_opaque(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Scalar SWAR Porter-Duff source-over with premultiplied source against
/// opaque dest. Computes channels of R+B together in one multiply, G in
/// another. ~30% faster than the naive 3-multiply scalar form.
#[inline(always)]
fn blend_over_swar(src_premul: u32, dst: u32) -> u32 {
    let sa = (src_premul >> 24) & 0xff;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return src_premul & 0x00ff_ffff;
    }
    let inv = 255 - sa;
    // R and B share a u32: 00RR00BB.
    let rb = (dst & 0x00ff_00ff) * inv;
    let rb = ((rb + 0x0080_0080 + ((rb >> 8) & 0x00ff_00ff)) >> 8) & 0x00ff_00ff;
    // G alone.
    let g = ((dst >> 8) & 0xff) * inv;
    let g = ((g + 0x80 + (g >> 8)) >> 8) & 0xff;
    let dst_blended = rb | (g << 8);
    let src_rgb = src_premul & 0x00ff_ffff;
    // Premultiplied src guarantees src.rgb <= src.a, so adding to the
    // attenuated dst can't carry into the next channel.
    let out_rb = ((src_rgb & 0x00ff_00ff) + (dst_blended & 0x00ff_00ff)) & 0x00ff_00ff;
    let out_g = (((src_rgb >> 8) & 0xff) + ((dst_blended >> 8) & 0xff)) & 0xff;
    out_rb | (out_g << 8)
}

/// SWAR source-over with constant src across all lanes (translucent rect).
/// `src_v` is splat of `(src_premul & 0x00ff_ffff)`; `inv_v` is splat of
/// `(255 - src.a)`. 4 dst pixels per call.
#[inline(always)]
fn blend_over_simd_const_src_x4(src_v: u32x4, inv_v: u32x4, dst: u32x4) -> u32x4 {
    let mask_rb = u32x4::splat(0x00ff_00ff);
    let half_rb = u32x4::splat(0x0080_0080);
    let mask_g = u32x4::splat(0xff);

    let drb = (dst & mask_rb) * inv_v;
    let drb = ((drb + half_rb + ((drb >> 8) & mask_rb)) >> 8) & mask_rb;

    let dg = (dst >> 8) & mask_g;
    let dg = dg * inv_v;
    let dg = ((dg + u32x4::splat(0x80) + (dg >> 8)) >> 8) & mask_g;
    let dg = dg << 8;

    src_v + drb + dg
}

/// 256-bit version of `blend_over_simd_const_src_x4` — 8 dst pixels per call.
/// Lights up on AVX2 (~all x86_64 since 2013). Falls back gracefully on
/// older hardware via wide's runtime detection.
#[inline(always)]
fn blend_over_simd_const_src_x8(src_v: u32x8, inv_v: u32x8, dst: u32x8) -> u32x8 {
    let mask_rb = u32x8::splat(0x00ff_00ff);
    let half_rb = u32x8::splat(0x0080_0080);
    let mask_g = u32x8::splat(0xff);

    let drb = (dst & mask_rb) * inv_v;
    let drb = ((drb + half_rb + ((drb >> 8) & mask_rb)) >> 8) & mask_rb;

    let dg = (dst >> 8) & mask_g;
    let dg = dg * inv_v;
    let dg = ((dg + u32x8::splat(0x80) + (dg >> 8)) >> 8) & mask_g;
    let dg = dg << 8;

    src_v + drb + dg
}

/// SWAR source-over with **per-lane** src and inv_a — used by glyph blit
/// where each cached glyph pixel has its own premultiplied (a,r,g,b).
/// Branchless: produces correct result for sa==0 (returns dst) and sa==255
/// (returns src) without any conditional, so the loop fully vectorizes.
#[inline(always)]
fn blend_over_simd_var_src_x4(src: u32x4, dst: u32x4) -> u32x4 {
    let mask_byte = u32x4::splat(0xff);
    let mask_rb = u32x4::splat(0x00ff_00ff);
    let half_rb = u32x4::splat(0x0080_0080);
    let mask_rgb = u32x4::splat(0x00ff_ffff);

    let sa = (src >> 24) & mask_byte;
    let inv_v = u32x4::splat(255) - sa;
    let src_rgb = src & mask_rgb;

    let drb = (dst & mask_rb) * inv_v;
    let drb = ((drb + half_rb + ((drb >> 8) & mask_rb)) >> 8) & mask_rb;

    let dg = (dst >> 8) & mask_byte;
    let dg = dg * inv_v;
    let dg = ((dg + u32x4::splat(0x80) + (dg >> 8)) >> 8) & mask_byte;
    let dg = dg << 8;

    src_rgb + drb + dg
}

#[derive(Clone, Copy)]
struct ParsedQuad {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
    min_u: f32,
    min_v: f32,
    color: [f32; 4],
    color_layer: i32,
    mask_layer: i32,
    clip: [f32; 4],
}

#[inline(always)]
fn parse_quad(chunk: &[Vertex]) -> ParsedQuad {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_u = f32::INFINITY;
    let mut min_v = f32::INFINITY;
    for v in chunk {
        if v.pos[0] < min_x {
            min_x = v.pos[0];
        }
        if v.pos[1] < min_y {
            min_y = v.pos[1];
        }
        if v.pos[0] > max_x {
            max_x = v.pos[0];
        }
        if v.pos[1] > max_y {
            max_y = v.pos[1];
        }
        if v.uv[0] < min_u {
            min_u = v.uv[0];
        }
        if v.uv[1] < min_v {
            min_v = v.uv[1];
        }
    }
    let v0 = chunk[0];
    ParsedQuad {
        min_x,
        min_y,
        max_x,
        max_y,
        min_u,
        min_v,
        color: v0.color,
        color_layer: v0.layers[0],
        mask_layer: v0.layers[1],
        clip: v0.clip_rect,
    }
}

/// Snap quad bounds to integer pixels and clip to (clip_rect ∩ buffer).
/// Returns Some((x0,y0,x1,y1)) or None if fully clipped.
#[inline(always)]
fn snap_and_clip(q: &ParsedQuad, buf_w: i32, buf_h: i32) -> Option<(i32, i32, i32, i32)> {
    let mut x0 = q.min_x.round() as i32;
    let mut y0 = q.min_y.round() as i32;
    let mut x1 = q.max_x.round() as i32;
    let mut y1 = q.max_y.round() as i32;

    if q.clip[2] > 0.0 && q.clip[3] > 0.0 {
        let cx0 = q.clip[0].round() as i32;
        let cy0 = q.clip[1].round() as i32;
        let cx1 = (q.clip[0] + q.clip[2]).round() as i32;
        let cy1 = (q.clip[1] + q.clip[3]).round() as i32;
        if x0 < cx0 {
            x0 = cx0;
        }
        if y0 < cy0 {
            y0 = cy0;
        }
        if x1 > cx1 {
            x1 = cx1;
        }
        if y1 > cy1 {
            y1 = cy1;
        }
    }
    if x0 < 0 {
        x0 = 0;
    }
    if y0 < 0 {
        y0 = 0;
    }
    if x1 > buf_w {
        x1 = buf_w;
    }
    if y1 > buf_h {
        y1 = buf_h;
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some((x0, y0, x1, y1))
}

#[derive(Clone, Copy)]
struct PendingFill {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    packed: u32,
}

impl PendingFill {
    #[inline(always)]
    fn try_extend(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, packed: u32) -> bool {
        if self.packed == packed && self.y0 == y0 && self.y1 == y1 && self.x1 == x0 {
            self.x1 = x1;
            return true;
        }
        false
    }
}

#[inline]
fn flush_fill(buf: &mut [u32], buf_w: i32, p: &PendingFill) {
    let buf_w_us = buf_w as usize;
    for y in p.y0..p.y1 {
        let row_start = (y as usize) * buf_w_us + (p.x0 as usize);
        let row_end = (y as usize) * buf_w_us + (p.x1 as usize);
        buf[row_start..row_end].fill(p.packed);
    }
}

pub fn render_cpu(
    ctx: &mut CpuContext,
    renderer: &Renderer,
    cache: &mut CpuCache,
    background: Option<wgpu::Color>,
) {
    let vertices = renderer.vertices();

    // Frame skip.
    let frame_hash = {
        let mut h = rustc_hash::FxHasher::default();
        if let Some(c) = background {
            h.write_u64(c.r.to_bits());
            h.write_u64(c.g.to_bits());
            h.write_u64(c.b.to_bits());
            h.write_u64(c.a.to_bits());
        } else {
            h.write_u8(0);
        }
        let bytes: &[u8] = bytemuck::cast_slice(vertices);
        h.write(bytes);
        h.finish()
    };

    if cache.has_last && cache.last_frame_hash == frame_hash {
        return;
    }
    cache.last_frame_hash = frame_hash;
    cache.has_last = true;

    let buf_w = ctx.width_px as i32;
    let buf_h = ctx.height_px as i32;
    if buf_w == 0 || buf_h == 0 {
        return;
    }

    let mut buffer = match ctx.surface.buffer_mut() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("softbuffer buffer_mut failed: {e}");
            return;
        }
    };

    let bg_u32 = match background {
        Some(c) => pack_opaque(
            (c.r.clamp(0.0, 1.0) * 255.0) as u8,
            (c.g.clamp(0.0, 1.0) * 255.0) as u8,
            (c.b.clamp(0.0, 1.0) * 255.0) as u8,
        ),
        None => 0,
    };
    buffer.fill(bg_u32);

    if !vertices.is_empty() {
        let images = renderer.image_cache();
        let atlas_size = images.cpu_max_texture_size();
        let buf_slice: &mut [u32] = &mut buffer;

        let mut pending: Option<PendingFill> = None;

        let mut i = 0usize;
        while i + 5 < vertices.len() {
            let chunk = &vertices[i..i + 6];
            i += 6;

            let q = parse_quad(chunk);
            if q.max_x - q.min_x <= 0.0 || q.max_y - q.min_y <= 0.0 {
                continue;
            }

            let snapped = match snap_and_clip(&q, buf_w, buf_h) {
                Some(r) => r,
                None => continue,
            };
            let (x0, y0, x1, y1) = snapped;

            // Glyph?
            if q.mask_layer > 0 {
                if let Some(p) = pending.take() {
                    flush_fill(buf_slice, buf_w, &p);
                }
                draw_glyph(
                    buf_slice, buf_w, x0, y0, x1, y1, q.min_x, q.min_y, q.min_u, q.min_v,
                    q.color, images, atlas_size, cache,
                );
                continue;
            }

            // Color-atlas (image / color glyph): not implemented.
            if q.color_layer > 0 {
                if let Some(p) = pending.take() {
                    flush_fill(buf_slice, buf_w, &p);
                }
                continue;
            }

            // Solid quad.
            let r = (q.color[0].clamp(0.0, 1.0) * 255.0) as u8;
            let g = (q.color[1].clamp(0.0, 1.0) * 255.0) as u8;
            let b = (q.color[2].clamp(0.0, 1.0) * 255.0) as u8;
            let a = (q.color[3].clamp(0.0, 1.0) * 255.0) as u8;
            if a == 0 {
                continue;
            }

            if a == 255 {
                let packed = pack_opaque(r, g, b);
                if let Some(p) = pending.as_mut() {
                    if p.try_extend(x0, y0, x1, y1, packed) {
                        continue;
                    }
                    flush_fill(buf_slice, buf_w, p);
                }
                pending = Some(PendingFill {
                    x0,
                    y0,
                    x1,
                    y1,
                    packed,
                });
            } else {
                if let Some(p) = pending.take() {
                    flush_fill(buf_slice, buf_w, &p);
                }
                fill_translucent_simd(buf_slice, buf_w, x0, y0, x1, y1, r, g, b, a);
            }
        }

        if let Some(p) = pending.take() {
            flush_fill(buf_slice, buf_w, &p);
        }
    }

    if let Err(e) = buffer.present() {
        tracing::error!("softbuffer present failed: {e}");
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn fill_translucent_simd(
    buf: &mut [u32],
    buf_w: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) {
    let a_u = a as u32;
    let pr = (r as u32 * a_u + 127) / 255;
    let pg = (g as u32 * a_u + 127) / 255;
    let pb = (b as u32 * a_u + 127) / 255;
    let src_premul = pack_premul(pr as u8, pg as u8, pb as u8, a);
    let src_rgb = src_premul & 0x00ff_ffff;
    let inv = 255 - a_u;
    let src_v8 = u32x8::splat(src_rgb);
    let inv_v8 = u32x8::splat(inv);
    let src_v4 = u32x4::splat(src_rgb);
    let inv_v4 = u32x4::splat(inv);
    let mask_rgb_x8 = u32x8::splat(0x00ff_ffff);
    let mask_rgb_x4 = u32x4::splat(0x00ff_ffff);

    let buf_w_us = buf_w as usize;
    for y in y0..y1 {
        let row_start = (y as usize) * buf_w_us + (x0 as usize);
        let row_end = (y as usize) * buf_w_us + (x1 as usize);
        let row = &mut buf[row_start..row_end];

        // 256-bit chunks first.
        let mut chunks8 = row.chunks_exact_mut(8);
        for chunk in &mut chunks8 {
            let dst = u32x8::new([
                chunk[0], chunk[1], chunk[2], chunk[3], chunk[4], chunk[5], chunk[6],
                chunk[7],
            ]);
            let out = blend_over_simd_const_src_x8(src_v8, inv_v8, dst) & mask_rgb_x8;
            let arr = out.to_array();
            chunk.copy_from_slice(&arr);
        }
        let tail = chunks8.into_remainder();

        // 128-bit tail.
        let mut chunks4 = tail.chunks_exact_mut(4);
        for chunk in &mut chunks4 {
            let dst = u32x4::new([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let out = blend_over_simd_const_src_x4(src_v4, inv_v4, dst) & mask_rgb_x4;
            let arr = out.to_array();
            chunk.copy_from_slice(&arr);
        }
        // Scalar tail.
        for px in chunks4.into_remainder() {
            *px = blend_over_swar(src_premul, *px);
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn draw_glyph(
    buf: &mut [u32],
    buf_w: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    quad_min_x: f32,
    quad_min_y: f32,
    min_u: f32,
    min_v: f32,
    color: [f32; 4],
    images: &ImageCache,
    atlas_size: u16,
    cache: &mut CpuCache,
) {
    if atlas_size == 0 {
        return;
    }
    let atlas_size_f = atlas_size as f32;
    let u0_px = (min_u * atlas_size_f) as i32;
    let v0_px = (min_v * atlas_size_f) as i32;
    let q_x0 = quad_min_x.round() as i32;
    let q_y0 = quad_min_y.round() as i32;

    let g_w = (x1 - x0).max(0) as u16;
    let g_h = (y1 - y0).max(0) as u16;
    if g_w == 0 || g_h == 0 {
        return;
    }

    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u8;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u8;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u8;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u8;
    let color_packed = u32::from_le_bytes([r, g, b, a]);

    let atlas_x = (u0_px + (x0 - q_x0)).max(0) as u16;
    let atlas_y = (v0_px + (y0 - q_y0)).max(0) as u16;

    let key = GlyphKey {
        atlas_x,
        atlas_y,
        w: g_w,
        h: g_h,
        color: color_packed,
    };

    if let std::collections::hash_map::Entry::Vacant(e) = cache.glyphs.entry(key) {
        let mask = images.cpu_mask_atlas_buffer();
        if mask.is_empty() {
            return;
        }
        let atlas_w_us = atlas_size as usize;
        let r_u = r as u32;
        let g_u = g as u32;
        let b_u = b as u32;
        let a_u = a as u32;

        let mut pixels = vec![0u32; (g_w as usize) * (g_h as usize)];
        for yy in 0..g_h as usize {
            let src_y = (atlas_y as usize + yy).min(atlas_w_us - 1);
            let src_row = src_y * atlas_w_us;
            let dst_row = yy * (g_w as usize);
            for xx in 0..g_w as usize {
                let src_x = (atlas_x as usize + xx).min(atlas_w_us - 1);
                let m = mask[src_row + src_x] as u32;
                let pa = (m * a_u + 127) / 255;
                if pa == 0 {
                    continue;
                }
                let pr = (r_u * pa + 127) / 255;
                let pg = (g_u * pa + 127) / 255;
                let pb = (b_u * pa + 127) / 255;
                pixels[dst_row + xx] =
                    pack_premul(pr as u8, pg as u8, pb as u8, pa as u8);
            }
        }

        e.insert(CachedGlyph {
            pixels,
            w: g_w,
            h: g_h,
        });
    }

    let glyph = cache.glyphs.get(&key).unwrap();
    let buf_w_us = buf_w as usize;
    let g_w_us = glyph.w as usize;

    let mask_rgb_x4 = u32x4::splat(0x00ff_ffff);

    for yy in 0..glyph.h as usize {
        let dst_y = y0 as usize + yy;
        let dst_row_off = dst_y * buf_w_us + x0 as usize;
        let src_row_off = yy * g_w_us;
        let dst_row = &mut buf[dst_row_off..dst_row_off + g_w_us];
        let src_row = &glyph.pixels[src_row_off..src_row_off + g_w_us];

        // SIMD: 4 pixels at a time, branchless blend (handles sa==0/255
        // correctly as a side effect of the formula).
        let mut dst_chunks = dst_row.chunks_exact_mut(4);
        let mut src_chunks = src_row.chunks_exact(4);
        for (dchunk, schunk) in (&mut dst_chunks).zip(&mut src_chunks) {
            let dst = u32x4::new([dchunk[0], dchunk[1], dchunk[2], dchunk[3]]);
            let src = u32x4::new([schunk[0], schunk[1], schunk[2], schunk[3]]);
            let out = blend_over_simd_var_src_x4(src, dst) & mask_rgb_x4;
            let arr = out.to_array();
            dchunk.copy_from_slice(&arr);
        }
        // Scalar tail keeps the early-out branches.
        let dst_tail = dst_chunks.into_remainder();
        let src_tail = src_chunks.remainder();
        for (d, &s) in dst_tail.iter_mut().zip(src_tail) {
            *d = blend_over_swar(s, *d);
        }
    }
}
