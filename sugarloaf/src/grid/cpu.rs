// Copyright (c) 2023-present, Raphael Amorim.
//
// CPU backend for the grid renderer.
//
// Mirrors the GPU backends (`metal`, `vulkan`, `webgpu`) in scope and
// data layout, but rasterises directly into the softbuffer u32 pixel
// buffer instead of recording draw calls. Storage matches the Metal
// renderer cell-for-cell: one flat `Vec<CellBg>` indexed by
// `row * cols + col`, plus per-row `Vec<CellText>` slots (slot 0 =
// block-cursor cells, 1..=rows = content rows, last = non-block
// cursor) so the row-rebuild path in `frontends/rioterm/src/grid_emit`
// stays backend-agnostic.
//
// The atlases live in RAM. `CpuGridAtlas` packs glyph bitmaps into a
// shelf-allocated grayscale (R8) or color (RGBA8 premul) buffer with
// the same `AtlasAllocator` the GPU paths use, so the atlas slot
// coordinates round-trip across backends.
//
// Position math in `render` mirrors `grid.metal`'s vertex shaders so
// glyph placement matches the GPU paths pixel-for-pixel:
//   - cell origin = (col * cell_w + grid_padding.left,
//                    row * cell_h + grid_padding.top)
//   - glyph origin = cell_origin + (bearing_x, cell_h - bearing_y)

use rustc_hash::FxHashMap;

use super::atlas::{AtlasSlot, GlyphKey, RasterizedGlyph};
use super::cell::{CellBg, CellText, GridUniforms};
use crate::renderer::image_cache::atlas::AtlasAllocator;

/// Initial atlas side. 1024² bytes_per_pixel = 1 MiB grayscale,
/// 4 MiB color. Smaller than the Metal default (2048²) since CPU
/// builds are usually memory-constrained machines.
const ATLAS_SIZE: u16 = 1024;
const ATLAS_MAX_SIZE: u16 = 4096;

/// Slot 0 = block-cursor cells, slot `rows + 1` = non-block-cursor
/// cells. Matches the Metal layout in `metal::init_fg_rows`.
const CURSOR_ROW_SLOTS: usize = 2;

/// CPU-side glyph atlas. R8 for grayscale masks, RGBA8 premultiplied
/// for color emoji. Uses the same shelf allocator as the GPU atlases
/// so `AtlasSlot` coords round-trip.
pub struct CpuGridAtlas {
    pixels: Vec<u8>,
    side: u16,
    bytes_per_pixel: u8,
    allocator: AtlasAllocator,
    slots: FxHashMap<GlyphKey, AtlasSlot>,
}

impl CpuGridAtlas {
    fn new(bytes_per_pixel: u8) -> Self {
        let side = ATLAS_SIZE;
        let pixels =
            vec![0u8; (side as usize) * (side as usize) * (bytes_per_pixel as usize)];
        Self {
            pixels,
            side,
            bytes_per_pixel,
            allocator: AtlasAllocator::new(side, side),
            slots: FxHashMap::default(),
        }
    }

    pub fn new_grayscale() -> Self {
        Self::new(1)
    }

    pub fn new_color() -> Self {
        Self::new(4)
    }

    #[inline]
    pub fn lookup(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.slots.get(&key).copied()
    }

    pub fn insert(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if glyph.width == 0 || glyph.height == 0 {
            // Zero-sized glyphs (e.g. spaces) still need a cache entry
            // so the rasterizer doesn't keep producing them, but they
            // occupy no atlas space.
            let slot = AtlasSlot {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
                bearing_x: glyph.bearing_x,
                bearing_y: glyph.bearing_y,
            };
            self.slots.insert(key, slot);
            return Some(slot);
        }

        let (x, y) = self.allocator.allocate(glyph.width, glyph.height)?;
        let slot = AtlasSlot {
            x,
            y,
            w: glyph.width,
            h: glyph.height,
            bearing_x: glyph.bearing_x,
            bearing_y: glyph.bearing_y,
        };
        self.slots.insert(key, slot);
        self.write_pixels(
            x as usize,
            y as usize,
            glyph.width as usize,
            glyph.height as usize,
            glyph.bytes,
        );
        Some(slot)
    }

    fn write_pixels(&mut self, x: usize, y: usize, w: usize, h: usize, src: &[u8]) {
        let bpp = self.bytes_per_pixel as usize;
        let stride = self.side as usize * bpp;
        let row_bytes = w * bpp;
        for row in 0..h {
            let src_off = row * row_bytes;
            let dst_off = (y + row) * stride + x * bpp;
            self.pixels[dst_off..dst_off + row_bytes]
                .copy_from_slice(&src[src_off..src_off + row_bytes]);
        }
    }

    /// Double the atlas side, copying old pixels into the top-left of
    /// the new buffer. Existing `AtlasSlot`s stay valid because their
    /// `(x, y)` fall inside the unchanged old region. Returns `false`
    /// when already at `ATLAS_MAX_SIZE`.
    pub fn grow(&mut self) -> bool {
        let (old_w, old_h) = self.allocator.dimensions();
        if old_w >= ATLAS_MAX_SIZE {
            return false;
        }
        let new_side = old_w.saturating_mul(2).min(ATLAS_MAX_SIZE);
        if new_side <= old_w {
            return false;
        }
        let bpp = self.bytes_per_pixel as usize;
        let mut new_pixels = vec![0u8; (new_side as usize) * (new_side as usize) * bpp];

        let old_stride = old_w as usize * bpp;
        let new_stride = new_side as usize * bpp;
        for row in 0..old_h as usize {
            let src_off = row * old_stride;
            let dst_off = row * new_stride;
            new_pixels[dst_off..dst_off + old_stride]
                .copy_from_slice(&self.pixels[src_off..src_off + old_stride]);
        }
        self.pixels = new_pixels;
        self.side = new_side;
        self.allocator.grow_to(new_side, new_side);
        true
    }

    #[inline]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    #[inline]
    pub fn side(&self) -> u16 {
        self.side
    }
}

pub struct CpuGridRenderer {
    cols: u32,
    rows: u32,
    /// `cols * rows` flat. Indexed `row * cols + col`.
    bg_cells: Vec<CellBg>,
    /// Per-row fg storage with the same indexing scheme as the GPU
    /// backends — slot 0 holds the block cursor, 1..=rows hold content
    /// rows, slot `rows + 1` holds the non-block cursor decoration.
    fg_rows: Vec<Vec<CellText>>,
    atlas_grayscale: CpuGridAtlas,
    atlas_color: CpuGridAtlas,
    needs_full_rebuild: bool,
}

impl CpuGridRenderer {
    pub fn new(cols: u32, rows: u32) -> Self {
        Self {
            cols,
            rows,
            bg_cells: vec![CellBg::TRANSPARENT; bg_capacity(cols, rows)],
            fg_rows: init_fg_rows(rows),
            atlas_grayscale: CpuGridAtlas::new_grayscale(),
            atlas_color: CpuGridAtlas::new_color(),
            needs_full_rebuild: true,
        }
    }

    pub fn resize(&mut self, cols: u32, rows: u32) {
        if cols == self.cols && rows == self.rows {
            return;
        }
        self.cols = cols;
        self.rows = rows;
        self.bg_cells = vec![CellBg::TRANSPARENT; bg_capacity(cols, rows)];
        self.fg_rows = init_fg_rows(rows);
        // Fresh buffers = zero contents; emission path must rewrite
        // every row on the next frame even if no damage came in.
        self.needs_full_rebuild = true;
    }

    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            slot.clear();
            slot.extend_from_slice(fg);
        }

        if row >= self.rows {
            return;
        }
        let cols = self.cols as usize;
        let row_start = (row as usize) * cols;
        let row_len = cols.min(bg.len());
        let dst = &mut self.bg_cells[row_start..row_start + cols];
        dst[..row_len].copy_from_slice(&bg[..row_len]);
        for slot in &mut dst[row_len..] {
            *slot = CellBg::TRANSPARENT;
        }
    }

    pub fn clear_row(&mut self, row: u32) {
        let idx = (row as usize) + 1;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            slot.clear();
        }
        if row >= self.rows {
            return;
        }
        let cols = self.cols as usize;
        let row_start = (row as usize) * cols;
        for slot in &mut self.bg_cells[row_start..row_start + cols] {
            *slot = CellBg::TRANSPARENT;
        }
    }

    pub fn set_block_cursor(&mut self, cells: &[CellText]) {
        if let Some(slot) = self.fg_rows.first_mut() {
            slot.clear();
            slot.extend_from_slice(cells);
        }
    }

    pub fn set_non_block_cursor(&mut self, cells: &[CellText]) {
        let idx = self.fg_rows.len().saturating_sub(1);
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            slot.clear();
            slot.extend_from_slice(cells);
        }
    }

    pub fn clear_cursor(&mut self) {
        if let Some(slot) = self.fg_rows.first_mut() {
            slot.clear();
        }
        let last = self.fg_rows.len().saturating_sub(1);
        if last > 0 {
            if let Some(slot) = self.fg_rows.get_mut(last) {
                slot.clear();
            }
        }
    }

    #[inline]
    pub fn lookup_glyph(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_grayscale.lookup(key)
    }

    pub fn insert_glyph(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if let Some(slot) = self.atlas_grayscale.insert(key, glyph) {
            return Some(slot);
        }
        if self.atlas_grayscale.grow() {
            self.atlas_grayscale.insert(key, glyph)
        } else {
            None
        }
    }

    #[inline]
    pub fn lookup_glyph_color(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_color.lookup(key)
    }

    pub fn insert_glyph_color(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        if let Some(slot) = self.atlas_color.insert(key, glyph) {
            return Some(slot);
        }
        if self.atlas_color.grow() {
            self.atlas_color.insert(key, glyph)
        } else {
            None
        }
    }

    #[inline]
    pub fn needs_full_rebuild(&self) -> bool {
        self.needs_full_rebuild
    }

    #[inline]
    pub fn mark_full_rebuild_done(&mut self) {
        self.needs_full_rebuild = false;
    }

    /// Hash all state that affects what `render` will paint. The CPU
    /// rasterizer's frame-skip path uses this to short-circuit when
    /// the previous frame's output is still valid.
    pub fn hash_state<H: std::hash::Hasher>(&self, h: &mut H) {
        // bg_cells is a flat `Vec<CellBg>` — hash as raw bytes.
        h.write(bytemuck::cast_slice(self.bg_cells.as_slice()));
        // Per-row CellText bytes. Length-prefix so two adjacent rows
        // can't be confused with one wider row.
        for row in &self.fg_rows {
            h.write_usize(row.len());
            h.write(bytemuck::cast_slice(row.as_slice()));
        }
    }

    /// Paint the grid (bg cells + cursor + fg glyphs) into the
    /// caller's `0x00RRGGBB` u32 buffer. Mirrors the bg + text passes
    /// of `grid.metal`'s shaders, in the same draw order so glyphs
    /// composite correctly over their cell backgrounds.
    pub fn render(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
    ) {
        let cell_w = uniforms.cell_size[0];
        let cell_h = uniforms.cell_size[1];
        if cell_w <= 0.0 || cell_h <= 0.0 {
            return;
        }
        let cols = uniforms.grid_size[0];
        let rows = uniforms.grid_size[1];
        if cols == 0 || rows == 0 {
            return;
        }
        // grid_padding = (top, right, bottom, left) — same as the shader.
        let pad_top = uniforms.grid_padding[0];
        let pad_left = uniforms.grid_padding[3];

        let buf_w_i = buf_w as i32;
        let buf_h_i = buf_h as i32;
        let cursor_x = uniforms.cursor_pos[0];
        let cursor_y = uniforms.cursor_pos[1];
        let cursor_bg_active = uniforms.cursor_bg_color[3] > 0.0;
        let cursor_fg_active = uniforms.cursor_color[3] > 0.0;
        let cursor_bg = normalize_color(uniforms.cursor_bg_color);
        let cursor_fg = normalize_color(uniforms.cursor_color);

        // ---------- bg pass ----------
        let buf_cols = self.cols as usize;
        let row_count = (rows as usize).min(self.rows as usize);
        let col_count = (cols as usize).min(self.cols as usize);
        for row in 0..row_count {
            let row_off = row * buf_cols;
            for col in 0..col_count {
                let mut rgba = self.bg_cells[row_off + col].rgba;
                if cursor_bg_active && cursor_x == col as u32 && cursor_y == row as u32 {
                    rgba = cursor_bg;
                }
                if rgba[3] == 0 {
                    continue;
                }

                let x0 = (pad_left + (col as f32) * cell_w).round() as i32;
                let y0 = (pad_top + (row as f32) * cell_h).round() as i32;
                let x1 = (pad_left + ((col + 1) as f32) * cell_w).round() as i32;
                let y1 = (pad_top + ((row + 1) as f32) * cell_h).round() as i32;
                fill_rect(buf, buf_w_i, buf_h_i, x0, y0, x1, y1, rgba);
            }
        }

        // ---------- text pass ----------
        let mask = self.atlas_grayscale.pixels();
        let mask_side = self.atlas_grayscale.side as usize;
        let color_atlas = self.atlas_color.pixels();
        let color_side = self.atlas_color.side as usize;

        for fg in &self.fg_rows {
            for glyph in fg {
                let gw = glyph.glyph_size[0] as i32;
                let gh = glyph.glyph_size[1] as i32;
                if gw <= 0 || gh <= 0 {
                    continue;
                }

                // Cell origin in pixels (matches grid.metal:258 + 275-276).
                let cell_pos_x = (glyph.grid_pos[0] as f32) * cell_w + pad_left;
                let cell_pos_y = (glyph.grid_pos[1] as f32) * cell_h + pad_top;
                // Glyph origin (matches grid.metal:267-271). bearings.y is
                // measured from the cell bottom, so flip into top-down space.
                let glyph_x = (cell_pos_x + glyph.bearings[0] as f32) as i32;
                let glyph_y = (cell_pos_y + cell_h - glyph.bearings[1] as f32) as i32;

                let mut color = glyph.color;
                if cursor_fg_active
                    && (glyph.bools & CellText::BOOL_IS_CURSOR_GLYPH) == 0
                    && cursor_x == glyph.grid_pos[0] as u32
                    && cursor_y == glyph.grid_pos[1] as u32
                {
                    color = cursor_fg;
                }

                let ax = glyph.glyph_pos[0] as usize;
                let ay = glyph.glyph_pos[1] as usize;

                if glyph.atlas == CellText::ATLAS_COLOR {
                    blit_color(
                        buf,
                        buf_w_i,
                        buf_h_i,
                        glyph_x,
                        glyph_y,
                        gw,
                        gh,
                        color_atlas,
                        color_side,
                        ax,
                        ay,
                    );
                } else {
                    blit_mask(
                        buf, buf_w_i, buf_h_i, glyph_x, glyph_y, gw, gh, mask, mask_side,
                        ax, ay, color,
                    );
                }
            }
        }
    }
}

#[inline]
fn bg_capacity(cols: u32, rows: u32) -> usize {
    (cols as usize) * (rows as usize)
}

#[inline]
fn init_fg_rows(rows: u32) -> Vec<Vec<CellText>> {
    (0..(rows as usize + CURSOR_ROW_SLOTS))
        .map(|_| Vec::new())
        .collect()
}

#[inline]
fn normalize_color(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

#[inline]
fn premul(c: [u8; 4]) -> [u8; 4] {
    let a = c[3] as u32;
    if a == 255 {
        return c;
    }
    if a == 0 {
        return [0, 0, 0, 0];
    }
    [
        ((c[0] as u32 * a + 127) / 255) as u8,
        ((c[1] as u32 * a + 127) / 255) as u8,
        ((c[2] as u32 * a + 127) / 255) as u8,
        c[3],
    ]
}

#[inline]
fn pack_opaque(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

/// Premultiplied source-over against a 0x00RRGGBB destination.
#[inline]
fn blend_over(src: [u8; 4], dst: u32) -> u32 {
    let sa = src[3] as u32;
    if sa == 0 {
        return dst;
    }
    if sa == 255 {
        return pack_opaque(src[0], src[1], src[2]);
    }
    let inv = 255 - sa;
    let dr = (dst >> 16) & 0xff;
    let dg = (dst >> 8) & 0xff;
    let db = dst & 0xff;
    // src is already premultiplied — add to attenuated dst.
    let or = src[0] as u32 + (dr * inv + 127) / 255;
    let og = src[1] as u32 + (dg * inv + 127) / 255;
    let ob = src[2] as u32 + (db * inv + 127) / 255;
    pack_opaque(or.min(255) as u8, og.min(255) as u8, ob.min(255) as u8)
}

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    buf: &mut [u32],
    buf_w: i32,
    buf_h: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    rgba: [u8; 4],
) {
    let x0 = x0.max(0);
    let y0 = y0.max(0);
    let x1 = x1.min(buf_w);
    let y1 = y1.min(buf_h);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let pre = premul(rgba);
    let stride = buf_w as usize;
    if pre[3] == 255 {
        let opaque = pack_opaque(pre[0], pre[1], pre[2]);
        for y in y0..y1 {
            let row_start = (y as usize) * stride + (x0 as usize);
            let row_end = (y as usize) * stride + (x1 as usize);
            buf[row_start..row_end].fill(opaque);
        }
    } else {
        for y in y0..y1 {
            let row_off = (y as usize) * stride;
            for x in x0..x1 {
                let idx = row_off + (x as usize);
                buf[idx] = blend_over(pre, buf[idx]);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_mask(
    buf: &mut [u32],
    buf_w: i32,
    buf_h: i32,
    glyph_x: i32,
    glyph_y: i32,
    gw: i32,
    gh: i32,
    atlas: &[u8],
    atlas_side: usize,
    ax: usize,
    ay: usize,
    color: [u8; 4],
) {
    if color[3] == 0 {
        return;
    }
    let stride = buf_w as usize;
    // Clip glyph rect to buffer + atlas bounds in one step.
    let x_start = glyph_x.max(0);
    let y_start = glyph_y.max(0);
    let x_end = (glyph_x + gw).min(buf_w);
    let y_end = (glyph_y + gh).min(buf_h);
    if x_end <= x_start || y_end <= y_start {
        return;
    }
    let r = color[0] as u32;
    let g = color[1] as u32;
    let b = color[2] as u32;
    let ca = color[3] as u32;

    for dst_y in y_start..y_end {
        let src_y = (dst_y - glyph_y) as usize + ay;
        if src_y >= atlas_side {
            continue;
        }
        let atlas_row = src_y * atlas_side;
        let buf_row = (dst_y as usize) * stride;
        for dst_x in x_start..x_end {
            let src_x = (dst_x - glyph_x) as usize + ax;
            if src_x >= atlas_side {
                continue;
            }
            let m = atlas[atlas_row + src_x] as u32;
            if m == 0 {
                continue;
            }
            // mask alpha × text alpha → premultiplied src
            let a = (m * ca + 127) / 255;
            if a == 0 {
                continue;
            }
            let pr = (r * a + 127) / 255;
            let pg = (g * a + 127) / 255;
            let pb = (b * a + 127) / 255;
            let src = [pr as u8, pg as u8, pb as u8, a as u8];
            let idx = buf_row + (dst_x as usize);
            buf[idx] = blend_over(src, buf[idx]);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn blit_color(
    buf: &mut [u32],
    buf_w: i32,
    buf_h: i32,
    glyph_x: i32,
    glyph_y: i32,
    gw: i32,
    gh: i32,
    atlas: &[u8],
    atlas_side: usize,
    ax: usize,
    ay: usize,
) {
    let stride = buf_w as usize;
    let x_start = glyph_x.max(0);
    let y_start = glyph_y.max(0);
    let x_end = (glyph_x + gw).min(buf_w);
    let y_end = (glyph_y + gh).min(buf_h);
    if x_end <= x_start || y_end <= y_start {
        return;
    }
    for dst_y in y_start..y_end {
        let src_y = (dst_y - glyph_y) as usize + ay;
        if src_y >= atlas_side {
            continue;
        }
        let atlas_row = src_y * atlas_side * 4;
        let buf_row = (dst_y as usize) * stride;
        for dst_x in x_start..x_end {
            let src_x = (dst_x - glyph_x) as usize + ax;
            if src_x >= atlas_side {
                continue;
            }
            let off = atlas_row + src_x * 4;
            let r = atlas[off];
            let g = atlas[off + 1];
            let b = atlas[off + 2];
            let a = atlas[off + 3];
            if a == 0 {
                continue;
            }
            // Atlas already holds premultiplied RGBA (color emoji
            // rasterizer convention).
            let src = [r, g, b, a];
            let idx = buf_row + (dst_x as usize);
            buf[idx] = blend_over(src, buf[idx]);
        }
    }
}
