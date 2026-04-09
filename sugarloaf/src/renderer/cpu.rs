// Copyright (c) 2023-present, Raphael Amorim.
//
// CPU rasterization pipeline.
//
// Writes directly into softbuffer's `&mut [u32]` (0x00RRGGBB) — no
// intermediate pixmap, no pixel format conversion at present time.
//
// Hot paths:
//   - **Solid opaque rect** (terminal cell backgrounds): row-by-row
//     `slice::fill` with the packed u32 color. ~1 cycle/pixel.
//   - **Glyph blit**: cached glyphs are stored as a packed `Vec<u32>` of
//     premultiplied (a<<24)|(r<<16)|(g<<8)|b. Per-pixel source-over blend
//     into the dest u32 buffer. No tiny-skia overhead.
//
// Frame skip: hash of (background, vertex bytes) compared to last frame;
// if equal, skip work and don't even acquire the softbuffer buffer.
//
// v1 limitations: monochrome glyphs only (color-atlas glyphs / images
// not implemented), no per-corner radii / borders / dashed-curly underlines.

use crate::context::cpu::CpuContext;
use crate::renderer::compositor::Vertex;
use crate::renderer::image_cache::ImageCache;
use crate::renderer::Renderer;
use rustc_hash::FxHashMap;
use std::hash::Hasher;

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
    /// Length = w * h. Each entry is premultiplied (a<<24)|(r<<16)|(g<<8)|b.
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

pub fn render_cpu(
    ctx: &mut CpuContext,
    renderer: &Renderer,
    cache: &mut CpuCache,
    background: Option<wgpu::Color>,
) {
    let vertices = renderer.vertices();

    // ---- Frame skip ----
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

    let width = ctx.width_px as i32;
    let height = ctx.height_px as i32;
    if width == 0 || height == 0 {
        return;
    }

    let mut buffer = match ctx.surface.buffer_mut() {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("softbuffer buffer_mut failed: {e}");
            return;
        }
    };

    // ---- Background fill ----
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

        let mut i = 0usize;
        while i + 5 < vertices.len() {
            let chunk = &vertices[i..i + 6];
            i += 6;
            draw_quad(buf_slice, width, height, chunk, images, atlas_size, cache);
        }
    }

    if let Err(e) = buffer.present() {
        tracing::error!("softbuffer present failed: {e}");
    }
}

#[inline]
fn draw_quad(
    buf: &mut [u32],
    buf_w: i32,
    buf_h: i32,
    chunk: &[Vertex],
    images: &ImageCache,
    atlas_size: u16,
    cache: &mut CpuCache,
) {
    // Reconstruct quad bounds and uv bounds from the 6 triangle vertices.
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut min_u = f32::INFINITY;
    let mut min_v = f32::INFINITY;
    let mut max_u = f32::NEG_INFINITY;
    let mut max_v = f32::NEG_INFINITY;
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
        if v.uv[0] > max_u {
            max_u = v.uv[0];
        }
        if v.uv[1] > max_v {
            max_v = v.uv[1];
        }
    }

    let qw = max_x - min_x;
    let qh = max_y - min_y;
    if qw <= 0.0 || qh <= 0.0 {
        return;
    }

    let v0 = chunk[0];
    let mask_layer = v0.layers[1];
    let color_layer = v0.layers[0];
    let color = v0.color;

    // Snap to integer pixel coordinates.
    let mut x0 = min_x.round() as i32;
    let mut y0 = min_y.round() as i32;
    let mut x1 = max_x.round() as i32;
    let mut y1 = max_y.round() as i32;

    // Apply clip rect.
    let clip = v0.clip_rect;
    if clip[2] > 0.0 && clip[3] > 0.0 {
        let cx0 = clip[0].round() as i32;
        let cy0 = clip[1].round() as i32;
        let cx1 = (clip[0] + clip[2]).round() as i32;
        let cy1 = (clip[1] + clip[3]).round() as i32;
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

    // Clip to buffer bounds.
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
        return;
    }

    if mask_layer > 0 {
        draw_glyph(
            buf, buf_w, buf_h, x0, y0, x1, y1, min_x, min_y, min_u, min_v, color,
            images, atlas_size, cache,
        );
        return;
    }

    if color_layer > 0 {
        // Color atlas glyph / image — not implemented in v1.
        return;
    }

    // Solid quad.
    let r = (color[0].clamp(0.0, 1.0) * 255.0) as u8;
    let g = (color[1].clamp(0.0, 1.0) * 255.0) as u8;
    let b = (color[2].clamp(0.0, 1.0) * 255.0) as u8;
    let a = (color[3].clamp(0.0, 1.0) * 255.0) as u8;

    if a == 0 {
        return;
    }

    if a == 255 {
        // Fast path: opaque axis-aligned rect — straight slice fills.
        let packed = pack_opaque(r, g, b);
        let buf_w_us = buf_w as usize;
        for y in y0..y1 {
            let row_start = (y as usize) * buf_w_us + (x0 as usize);
            let row_end = (y as usize) * buf_w_us + (x1 as usize);
            buf[row_start..row_end].fill(packed);
        }
    } else {
        // Translucent quad: per-pixel source-over blend (premultiplied src).
        let inv_a = 255 - a as u32;
        let pr = ((r as u32) * (a as u32) + 127) / 255;
        let pg = ((g as u32) * (a as u32) + 127) / 255;
        let pb = ((b as u32) * (a as u32) + 127) / 255;
        let buf_w_us = buf_w as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let idx = (y as usize) * buf_w_us + (x as usize);
                let dst = buf[idx];
                let dr = (dst >> 16) & 0xff;
                let dg = (dst >> 8) & 0xff;
                let db = dst & 0xff;
                let or = pr + (dr * inv_a + 127) / 255;
                let og = pg + (dg * inv_a + 127) / 255;
                let ob = pb + (db * inv_a + 127) / 255;
                buf[idx] = (or << 16) | (og << 8) | ob;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn draw_glyph(
    buf: &mut [u32],
    buf_w: i32,
    _buf_h: i32,
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

    // Compute the original (un-clipped) glyph footprint in atlas pixels so
    // the cache key is stable across clip variation.
    let u0_px = (min_u * atlas_size_f) as i32;
    let v0_px = (min_v * atlas_size_f) as i32;
    let q_x0 = quad_min_x.round() as i32;
    let q_y0 = quad_min_y.round() as i32;
    // Glyph natural size from min uv + (clipped quad size lower-bounded).
    // We need the *unclipped* size so the key is stable. Re-derive from
    // x1-x0 + clip offset is unreliable; instead, we only key on atlas
    // coords + clipped size, and accept that the cache may store one entry
    // per distinct clipped variant — in practice clipping rarely cuts a
    // glyph mid-cell.
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

    // Atlas read offset = atlas top-left + (clip offset on the dest side).
    let atlas_x = (u0_px + (x0 - q_x0)).max(0) as u16;
    let atlas_y = (v0_px + (y0 - q_y0)).max(0) as u16;

    let key = GlyphKey {
        atlas_x,
        atlas_y,
        w: g_w,
        h: g_h,
        color: color_packed,
    };

    if !cache.glyphs.contains_key(&key) {
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
                    // pixels[..] = 0 already
                    continue;
                }
                let pr = (r_u * pa + 127) / 255;
                let pg = (g_u * pa + 127) / 255;
                let pb = (b_u * pa + 127) / 255;
                pixels[dst_row + xx] =
                    pack_premul(pr as u8, pg as u8, pb as u8, pa as u8);
            }
        }

        cache.glyphs.insert(
            key,
            CachedGlyph {
                pixels,
                w: g_w,
                h: g_h,
            },
        );
    }

    let glyph = cache.glyphs.get(&key).unwrap();
    let buf_w_us = buf_w as usize;
    let g_w_us = glyph.w as usize;

    // Source-over blend: dst = src.rgb + dst.rgb * (255 - src.a) / 255.
    // src is premultiplied; dst has no alpha (treated as opaque).
    for yy in 0..glyph.h as usize {
        let dst_y = y0 as usize + yy;
        let dst_row = dst_y * buf_w_us;
        let src_row = yy * g_w_us;
        for xx in 0..glyph.w as usize {
            let s = glyph.pixels[src_row + xx];
            let sa = (s >> 24) & 0xff;
            if sa == 0 {
                continue;
            }
            let dst_idx = dst_row + (x0 as usize + xx);
            if sa == 255 {
                buf[dst_idx] = s & 0x00ff_ffff;
                continue;
            }
            let sr = (s >> 16) & 0xff;
            let sg = (s >> 8) & 0xff;
            let sb = s & 0xff;
            let inv = 255 - sa;
            let d = buf[dst_idx];
            let dr = (d >> 16) & 0xff;
            let dg = (d >> 8) & 0xff;
            let db = d & 0xff;
            let or = sr + (dr * inv + 127) / 255;
            let og = sg + (dg * inv + 127) / 255;
            let ob = sb + (db * inv + 127) / 255;
            buf[dst_idx] = (or << 16) | (og << 8) | ob;
        }
    }
}
