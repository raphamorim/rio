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
                page: 0,
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
            page: 0,
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

    pub fn clear(&mut self) {
        self.allocator.clear();
        self.slots.clear();
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
    /// Rows whose cells changed since the last paint, for a frame that
    /// repaints only those.
    damaged: Vec<bool>,
    /// Every row must be repainted: fresh buffers or a new atlas.
    damaged_full: bool,
    /// The uniforms of the last paint, to tell a cursor move (two rows)
    /// from a change that touches every cell.
    painted: Option<GridUniforms>,
    /// What each row holds, whichever row it is on, and what each held
    /// at the last paint: a frame whose rows are the last one's moved up
    /// is a scroll.
    keys: Vec<u64>,
    painted_keys: Vec<u64>,
    /// Rows the cursor was drawn on at the last paint.
    painted_cursor: Vec<u32>,
}

/// What changed in a grid since its last paint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridDamage {
    None,
    /// The rows written to the list passed to
    /// [`CpuGridRenderer::take_damage`].
    Rows,
    /// The last paint's rows moved up this many rows; then the rows in
    /// the list, as with `Rows`.
    Scroll(u32),
    Full,
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
            damaged: vec![false; rows as usize],
            damaged_full: true,
            painted: None,
            keys: vec![0; rows as usize],
            painted_keys: vec![0; rows as usize],
            painted_cursor: Vec::new(),
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
        self.damaged = vec![false; rows as usize];
        self.damaged_full = true;
        self.keys = vec![0; rows as usize];
        self.painted_keys = vec![0; rows as usize];
    }

    fn damage_row(&mut self, row: usize) {
        if let Some(flag) = self.damaged.get_mut(row) {
            *flag = true;
        }
    }

    pub fn write_row(&mut self, row: u32, bg: &[CellBg], fg: &[CellText]) {
        let idx = (row as usize) + 1;
        let mut changed = false;
        if let Some(slot) = self.fg_rows.get_mut(idx) {
            if slot.as_slice() != fg {
                slot.clear();
                slot.extend_from_slice(fg);
                changed = true;
            }
        }

        if row < self.rows {
            let cols = self.cols as usize;
            let row_start = (row as usize) * cols;
            let row_len = cols.min(bg.len());
            let dst = &mut self.bg_cells[row_start..row_start + cols];
            if dst[..row_len] != bg[..row_len]
                || dst[row_len..].iter().any(|c| *c != CellBg::TRANSPARENT)
            {
                dst[..row_len].copy_from_slice(&bg[..row_len]);
                dst[row_len..].fill(CellBg::TRANSPARENT);
                changed = true;
            }
        }
        if changed {
            self.damage_row(row as usize);
            if let Some(key) = self.keys.get_mut(row as usize) {
                *key = row_key(bg, fg);
            }
        }
    }

    pub fn clear_row(&mut self, row: u32) {
        self.write_row(row, &[], &[]);
    }

    pub fn set_cursor(&mut self, block: &[CellText], non_block: &[CellText]) {
        let last = self.fg_rows.len().saturating_sub(1);
        for (slot, cells) in [(0, block), (last, non_block)] {
            let Some(current) = self.fg_rows.get_mut(slot) else {
                continue;
            };
            if current.as_slice() == cells {
                continue;
            }
            for cell in current.iter().chain(cells) {
                if let Some(flag) = self.damaged.get_mut(cell.grid_pos[1] as usize) {
                    *flag = true;
                }
            }
            current.clear();
            current.extend_from_slice(cells);
        }
    }

    /// What changed since the last call: nothing, the rows pushed to
    /// `rows`, or everything. `uniforms` are the ones this paint uses;
    /// a moved or recolored cursor damages the rows it left and
    /// entered, any other uniform change the whole grid.
    pub fn take_damage(
        &mut self,
        uniforms: &GridUniforms,
        rows: &mut Vec<u32>,
    ) -> GridDamage {
        rows.clear();
        let mut full = std::mem::take(&mut self.damaged_full);
        match self.painted.replace(*uniforms) {
            None => full = true,
            Some(last) => {
                let without_cursor = |u: &GridUniforms| {
                    let mut u = *u;
                    u.cursor_pos = [0; 2];
                    u.cursor_color = [0.0; 4];
                    u.cursor_bg_color = [0.0; 4];
                    u
                };
                if bytemuck::bytes_of(&without_cursor(&last))
                    != bytemuck::bytes_of(&without_cursor(uniforms))
                {
                    full = true;
                } else if last.cursor_pos != uniforms.cursor_pos
                    || last.cursor_color != uniforms.cursor_color
                    || last.cursor_bg_color != uniforms.cursor_bg_color
                {
                    self.damage_row(last.cursor_pos[1] as usize);
                    self.damage_row(uniforms.cursor_pos[1] as usize);
                }
            }
        }
        let cursor = self.cursor_rows(uniforms);
        if full {
            self.damaged.fill(false);
            self.painted_keys.copy_from_slice(&self.keys);
            self.painted_cursor = cursor;
            return GridDamage::Full;
        }
        for (row, flag) in self.damaged.iter_mut().enumerate() {
            if std::mem::take(flag) {
                rows.push(row as u32);
            }
        }
        let total = self.keys.len();
        let scroll = if rows.len() * 4 >= total {
            self.find_scroll(rows)
        } else {
            None
        };
        let damage = match scroll {
            Some(shift) => {
                rows.clear();
                let shift = shift as usize;
                for row in 0..total {
                    let moved = self.painted_keys.get(row + shift);
                    if row == 0 || moved != Some(&self.keys[row]) {
                        rows.push(row as u32);
                    }
                }
                for &row in &self.painted_cursor {
                    if let Some(row) = (row as usize).checked_sub(shift) {
                        rows.push(row as u32);
                    }
                }
                rows.extend_from_slice(&cursor);
                rows.sort_unstable();
                rows.dedup();
                GridDamage::Scroll(shift as u32)
            }
            None if rows.is_empty() => GridDamage::None,
            None => GridDamage::Rows,
        };
        self.painted_keys.copy_from_slice(&self.keys);
        self.painted_cursor = cursor;
        damage
    }

    /// Rows the cursor draws on with `uniforms`: its sprites and the cell
    /// a block cursor tints.
    fn cursor_rows(&self, uniforms: &GridUniforms) -> Vec<u32> {
        let last = self.fg_rows.len().saturating_sub(1);
        let mut rows: Vec<u32> = [0, last]
            .iter()
            .filter_map(|&slot| self.fg_rows.get(slot))
            .flatten()
            .map(|cell| cell.grid_pos[1] as u32)
            .collect();
        if uniforms.cursor_pos[1] != u32::MAX {
            rows.push(uniforms.cursor_pos[1]);
        }
        rows
    }

    /// How many rows the content moved up since the last paint, when most
    /// of it did. Anchored on a damaged row that differs from the row
    /// below it, so runs of blank rows don't decide.
    fn find_scroll(&self, damaged: &[u32]) -> Option<u32> {
        let total = self.keys.len();
        let anchor = damaged
            .iter()
            .map(|&r| r as usize)
            .find(|&r| r + 1 < total && self.keys[r] != self.keys[r + 1])?;
        let key = self.keys[anchor];
        let shift =
            (anchor + 1..total).find(|&old| self.painted_keys[old] == key)? - anchor;
        let kept = (0..total - shift)
            .filter(|&row| self.keys[row] == self.painted_keys[row + shift])
            .count();
        (kept * 4 >= (total - shift) * 3).then_some(shift as u32)
    }

    #[inline]
    pub fn lookup_glyph(&self, key: GlyphKey) -> Option<AtlasSlot> {
        self.atlas_grayscale.lookup(key)
    }

    /// Drop every cached glyph and force a full rebuild. Called when
    /// the font library is swapped, since the new library reuses font ids.
    pub fn clear_atlas(&mut self) {
        self.atlas_grayscale.clear();
        self.atlas_color.clear();
        self.needs_full_rebuild = true;
        self.damaged_full = true;
    }

    pub fn insert_glyph(
        &mut self,
        key: GlyphKey,
        glyph: RasterizedGlyph<'_>,
    ) -> Option<AtlasSlot> {
        let mut cleared = false;
        loop {
            if let Some(slot) = self.atlas_grayscale.insert(key, glyph) {
                return Some(slot);
            }
            if self.atlas_grayscale.grow() {
                continue;
            }
            if cleared {
                return None;
            }
            self.atlas_grayscale.clear();
            self.needs_full_rebuild = true;
            self.damaged_full = true;
            cleared = true;
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
        let mut cleared = false;
        loop {
            if let Some(slot) = self.atlas_color.insert(key, glyph) {
                return Some(slot);
            }
            if self.atlas_color.grow() {
                continue;
            }
            if cleared {
                return None;
            }
            self.atlas_color.clear();
            self.needs_full_rebuild = true;
            self.damaged_full = true;
            cleared = true;
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

    /// Force the next frame to rebuild every row, even when nothing dirtied
    /// the cells. Used when state outside the grid (e.g. the color theme)
    /// changed and every cell must re-resolve.
    #[inline]
    pub fn request_full_rebuild(&mut self) {
        self.needs_full_rebuild = true;
    }

    /// Paint the grid (bg cells + cursor + fg glyphs) into the
    /// caller's `0x00RRGGBB` u32 buffer. Mirrors the bg + text passes
    /// of `grid.metal`'s shaders, in the same draw order so glyphs
    /// composite correctly over their cell backgrounds.
    /// Paint the cell-bg pass into `buf`. Pair with `render_text`,
    /// with any `kitty_below_text` images composited in between.
    pub fn render_bg(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
    ) {
        self.render_bg_band(buf, buf_w, buf_h, uniforms, 0, buf_h);
    }

    /// [`Self::render_bg`] limited to the pixel rows `y0..y1`.
    pub fn render_bg_band(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
        y0: u32,
        y1: u32,
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
        let pad_top = uniforms.grid_padding[0];
        let pad_left = uniforms.grid_padding[3];

        let buf_w_i = buf_w as i32;
        let clip = (y0 as i32, y1.min(buf_h) as i32);
        let cursor_x = uniforms.cursor_pos[0];
        let cursor_y = uniforms.cursor_pos[1];
        let cursor_bg_active = uniforms.cursor_bg_color[3] > 0.0;
        let cursor_bg = normalize_color(uniforms.cursor_bg_color);

        let buf_cols = self.cols as usize;
        let row_count = (rows as usize).min(self.rows as usize);
        let col_count = (cols as usize).min(self.cols as usize);
        for row in 0..row_count {
            let y0 = (pad_top + (row as f32) * cell_h).round() as i32;
            let y1 = (pad_top + ((row + 1) as f32) * cell_h).round() as i32;
            if y1 <= clip.0 || y0 >= clip.1 {
                continue;
            }
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
                let x1 = (pad_left + ((col + 1) as f32) * cell_w).round() as i32;
                fill_rect(buf, buf_w_i, clip, x0, y0, x1, y1, rgba);
            }
        }
    }

    /// Paint the cell-text pass into `buf`. Walks `fg_rows` and blits
    /// each glyph's atlas slot at the cell origin computed from
    /// `grid_pos + bearings`.
    pub fn render_text(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
    ) {
        self.render_text_band(buf, buf_w, buf_h, uniforms, 0, buf_h);
    }

    /// [`Self::render_text`] limited to the pixel rows `y0..y1`. Glyphs
    /// of the rows next to the band are drawn too, clipped, since they
    /// can reach into it.
    pub fn render_text_band(
        &self,
        buf: &mut [u32],
        buf_w: u32,
        buf_h: u32,
        uniforms: &GridUniforms,
        y0: u32,
        y1: u32,
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
        let pad_top = uniforms.grid_padding[0];
        let pad_left = uniforms.grid_padding[3];

        let buf_w_i = buf_w as i32;
        let clip = (y0 as i32, y1.min(buf_h) as i32);
        // Content rows that can reach the band: those it covers and one
        // more on each side, for glyphs taller than their cell.
        let first_row = (((clip.0 as f32 - pad_top) / cell_h).floor() as i64 - 1).max(0);
        let last_row = ((clip.1 as f32 - pad_top) / cell_h).ceil() as i64 + 1;
        let cursor_slot = self.fg_rows.len().saturating_sub(1);
        let cursor_x = uniforms.cursor_pos[0];
        let cursor_y = uniforms.cursor_pos[1];
        let cursor_fg_active = uniforms.cursor_color[3] > 0.0;
        let cursor_fg = normalize_color(uniforms.cursor_color);

        let mask = self.atlas_grayscale.pixels();
        let mask_side = self.atlas_grayscale.side as usize;
        let color_atlas = self.atlas_color.pixels();
        let color_side = self.atlas_color.side as usize;

        for (slot, fg) in self.fg_rows.iter().enumerate() {
            if slot != 0 && slot != cursor_slot {
                let row = slot as i64 - 1;
                if row < first_row || row > last_row {
                    continue;
                }
            }
            for glyph in fg {
                let gw = glyph.glyph_size[0] as i32;
                let gh = glyph.glyph_size[1] as i32;
                if gw <= 0 || gh <= 0 {
                    continue;
                }

                let cell_pos_x = (glyph.grid_pos[0] as f32) * cell_w + pad_left;
                let cell_pos_y = (glyph.grid_pos[1] as f32) * cell_h + pad_top;
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
                        clip,
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
                        buf, buf_w_i, clip, glyph_x, glyph_y, gw, gh, mask, mask_side,
                        ax, ay, color,
                    );
                }
            }
        }
    }
}

/// What a row holds, independent of which row it is on.
fn row_key(bg: &[CellBg], fg: &[CellText]) -> u64 {
    use std::hash::Hasher;
    let mut h = rustc_hash::FxHasher::default();
    h.write(bytemuck::cast_slice(bg));
    for cell in fg {
        let mut cell = *cell;
        cell.grid_pos[1] = 0;
        h.write(bytemuck::bytes_of(&cell));
    }
    h.finish()
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

use crate::premul::{blend_premul_over as blend_over, pack_opaque};

#[allow(clippy::too_many_arguments)]
fn fill_rect(
    buf: &mut [u32],
    buf_w: i32,
    clip: (i32, i32),
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    rgba: [u8; 4],
) {
    let x0 = x0.max(0);
    let y0 = y0.max(clip.0);
    let x1 = x1.min(buf_w);
    let y1 = y1.min(clip.1);
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
    clip: (i32, i32),
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
    let y_start = glyph_y.max(clip.0);
    let x_end = (glyph_x + gw).min(buf_w);
    let y_end = (glyph_y + gh).min(clip.1);
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
    clip: (i32, i32),
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
    let y_start = glyph_y.max(clip.0);
    let x_end = (glyph_x + gw).min(buf_w);
    let y_end = (glyph_y + gh).min(clip.1);
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

#[cfg(test)]
mod damage_tests {
    use super::*;
    use bytemuck::Zeroable;

    fn uniforms(cursor: [u32; 2]) -> GridUniforms {
        let mut u = GridUniforms::zeroed();
        u.cell_size = [10.0, 20.0];
        u.grid_size = [4, 5];
        u.cursor_pos = cursor;
        u
    }

    fn glyph(col: u16, row: u16) -> CellText {
        CellText {
            glyph_size: [1, 1],
            grid_pos: [col, row],
            color: [255; 4],
            ..CellText::default()
        }
    }

    fn painted(grid: &mut CpuGridRenderer, u: &GridUniforms) -> (GridDamage, Vec<u32>) {
        let mut rows = Vec::new();
        let damage = grid.take_damage(u, &mut rows);
        (damage, rows)
    }

    #[test]
    fn first_paint_and_resize_are_full() {
        let mut grid = CpuGridRenderer::new(4, 5);
        let u = uniforms([0, 0]);
        assert_eq!(painted(&mut grid, &u).0, GridDamage::Full);
        assert_eq!(painted(&mut grid, &u).0, GridDamage::None);
        grid.resize(4, 6);
        assert_eq!(painted(&mut grid, &u).0, GridDamage::Full);
    }

    #[test]
    fn only_rows_that_change_are_damaged() {
        let mut grid = CpuGridRenderer::new(4, 5);
        let u = uniforms([0, 0]);
        painted(&mut grid, &u);
        let red = [CellBg {
            rgba: [255, 0, 0, 255],
        }];
        grid.write_row(1, &red, &[glyph(0, 1)]);
        grid.write_row(3, &[], &[]);
        assert_eq!(painted(&mut grid, &u), (GridDamage::Rows, vec![1]));
        grid.write_row(1, &red, &[glyph(0, 1)]);
        assert_eq!(painted(&mut grid, &u).0, GridDamage::None, "same cells");
        grid.clear_row(1);
        assert_eq!(painted(&mut grid, &u), (GridDamage::Rows, vec![1]));
    }

    #[test]
    fn a_cursor_damages_the_rows_it_leaves_and_enters() {
        let mut grid = CpuGridRenderer::new(4, 5);
        painted(&mut grid, &uniforms([0, 0]));
        assert_eq!(
            painted(&mut grid, &uniforms([2, 3])),
            (GridDamage::Rows, vec![0, 3])
        );
        grid.set_cursor(&[], &[glyph(1, 2)]);
        assert_eq!(
            painted(&mut grid, &uniforms([2, 3])),
            (GridDamage::Rows, vec![2])
        );
        grid.set_cursor(&[], &[glyph(1, 4)]);
        assert_eq!(
            painted(&mut grid, &uniforms([2, 3])),
            (GridDamage::Rows, vec![2, 4])
        );
    }

    #[test]
    fn content_moved_up_is_a_scroll() {
        let mut grid = CpuGridRenderer::new(4, 5);
        let u = uniforms([u32::MAX, u32::MAX]);
        let line = |n: u8, row: u16| {
            let mut cell = glyph(0, row);
            cell.glyph_pos = [n as u32, 0];
            (
                [CellBg {
                    rgba: [n, 0, 0, 255],
                }],
                [cell],
            )
        };
        for row in 0..5u16 {
            let (bg, fg) = line(row as u8, row);
            grid.write_row(row as u32, &bg, &fg);
        }
        painted(&mut grid, &u);
        for row in 0..5u16 {
            let (bg, fg) = line(row as u8 + 2, row);
            grid.write_row(row as u32, &bg, &fg);
        }
        // Rows 0..3 hold what rows 2..5 held; row 0 is always repainted,
        // for what the old top row left in the padding above it.
        assert_eq!(
            painted(&mut grid, &u),
            (GridDamage::Scroll(2), vec![0, 3, 4])
        );
    }

    #[test]
    fn other_uniform_changes_and_a_new_atlas_are_full() {
        let mut grid = CpuGridRenderer::new(4, 5);
        let mut u = uniforms([0, 0]);
        painted(&mut grid, &u);
        u.cell_size = [11.0, 20.0];
        assert_eq!(painted(&mut grid, &u).0, GridDamage::Full);
        grid.clear_atlas();
        assert_eq!(painted(&mut grid, &u).0, GridDamage::Full);
    }
}
