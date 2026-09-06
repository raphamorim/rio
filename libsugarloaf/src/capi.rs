#![allow(clippy::missing_safety_doc)]

//! The C ABI. All calls must happen on the thread that owns the NSView
//! (the main/UI thread on macOS): `Sugarloaf` and `GridRenderer` own GPU
//! handles and are not `Send`.

use std::ffi::{c_char, c_void, CStr};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::NonNull;

use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};
// Color / style types the grid-emit palette resolves against. These are
// `rio_vt` re-exports; rio-grid names them via `rio_backend`, which
// re-exports the same crate, so they're the identical nominal types.
use rio_vt::config::colors::term::{List, TermColors, DIM_FACTOR};
use rio_vt::config::colors::{AnsiColor, ColorArray, ColorWGPU, Colors, NamedColor};
use rio_vt::crosswords::style::{Style, StyleFlags};
use sugarloaf::font::fonts::SugarloafFonts;
use sugarloaf::font::FontLibrary;
use sugarloaf::grid::atlas::{GlyphKey, RasterizedGlyph};
use sugarloaf::grid::cell::{CellBg, CellText};
use sugarloaf::grid::{GridRenderer, GridUniforms};
use sugarloaf::layout::RootStyle;
use sugarloaf::{
    kitty_image_key, Color, ColorType, Colorspace, GraphicData, GraphicDataEntry,
    GraphicId, GraphicOverlay, Sugarloaf, SugarloafBackend, SugarloafRenderer,
    SugarloafWindow, SugarloafWindowSize,
};

/// The renderer instance, boxed and handed to the host as an opaque pointer.
/// `'static`: the only borrow `Sugarloaf` carries is the raw window handle,
/// which is a bare pointer with no real lifetime.
///
/// Wraps `Sugarloaf` so a per-instance `GridGlyphRasterizer` (the CoreText
/// shaping / atlas caches the grid-emit path needs) and the font metrics
/// (`font_size` / `line_height` / `scale`, captured at `sl_new`) can ride
/// along. Derefs to the inner `Sugarloaf`, so every existing `sl_*` entry
/// point keeps calling `Sugarloaf` methods and reading its fields directly.
pub struct Sl {
    inner: Sugarloaf<'static>,
    rasterizer: rio_grid::GridGlyphRasterizer,
    font_size: f32,
    line_height: f32,
    scale: f32,
    /// The color theme the grid-emit path resolves cells against. Starts on
    /// Rio's default theme; the host overrides it with `sl_set_theme`.
    palette: HostPalette,
    /// Whether the last frame carried a selection, so the frame after it
    /// clears still repaints those rows to drop the tint.
    ///
    /// NOTE: this (and `had_search`) assume one grid per `Sl` — the model the
    /// `sl_render_surface` host uses (one NSView → one `Sl` → one grid). If a
    /// single `Sl` ever drives several grids via `sl_render_grids`, these
    /// one-frame-latch flags would need to move onto the grid.
    had_selection: bool,
    /// Same, for search highlights: the frame after the matches clear must
    /// repaint to drop the highlight from rows the dirty bits won't flag.
    had_search: bool,
    /// Padding (physical px) between the drawable edges and the cell grid:
    /// top, right, bottom, left. Fed into `GridUniforms.grid_padding` by
    /// `sl_render_surface`; hosts driving `sl_render_grid` directly pass
    /// their own uniforms and ignore this.
    grid_padding: [f32; 4],
    /// Bitmask of edges whose row/column background colors extend into the
    /// padding band (bit0 left, bit1 right, bit2 up, bit3 down).
    padding_extend: u32,
}

/// The selected column interval on visible row `y`, from the render state's
/// (already viewport-relative) selection. Bypasses rio_grid::row_selection_for,
/// which wants an absolute SelectionRange + display_offset.
fn row_selection_from(
    sel: Option<librio::ViewportSelection>,
    y: u16,
    cols: usize,
) -> Option<rio_grid::RowSelection> {
    let sel = sel?;
    if cols == 0 || y < sel.start_line || y > sel.end_line {
        return None;
    }
    let cols_max = cols.saturating_sub(1) as u16;
    if sel.is_block {
        // A block dragged right-to-left has start_col > end_col; order the
        // interval so `[lo, hi]` is never inverted (which would highlight
        // nothing).
        return Some(rio_grid::RowSelection {
            lo: sel.start_col.min(sel.end_col).min(cols_max),
            hi: sel.start_col.max(sel.end_col).min(cols_max),
        });
    }
    let lo = if y == sel.start_line {
        sel.start_col
    } else {
        0
    };
    let hi = if y == sel.end_line {
        sel.end_col
    } else {
        cols_max
    };
    Some(rio_grid::RowSelection {
        lo: lo.min(cols_max),
        hi: hi.min(cols_max),
    })
}

impl std::ops::Deref for Sl {
    type Target = Sugarloaf<'static>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl std::ops::DerefMut for Sl {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Integer cell metrics the host needs to turn a pixel size into cols/rows.
#[repr(C)]
pub struct SlCellMetrics {
    pub cell_width: u32,
    pub cell_height: u32,
    pub cell_baseline: u32,
}

/// A glyph's position + metrics in the atlas. A `#[repr(C)]` mirror of
/// sugarloaf's `AtlasSlot` (which is not itself `#[repr(C)]`), so the ABI
/// layout is fixed regardless of upstream field reordering.
#[repr(C)]
pub struct SlAtlasSlot {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub bearing_x: i16,
    pub bearing_y: i16,
    pub page: u8,
    pub _pad: u8,
}

#[cfg(target_os = "macos")]
fn default_backend() -> SugarloafBackend {
    SugarloafBackend::Metal
}
#[cfg(not(target_os = "macos"))]
fn default_backend() -> SugarloafBackend {
    // libsugarloaf targets macOS; the CPU backend keeps the workspace
    // building on other hosts (e.g. Linux CI) without a GPU.
    SugarloafBackend::Cpu
}

fn colorspace_from_u32(v: u32) -> Colorspace {
    match v {
        1 => Colorspace::DisplayP3,
        2 => Colorspace::Rec2020,
        _ => Colorspace::Srgb,
    }
}

unsafe fn slice_or_empty<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        std::slice::from_raw_parts(ptr, len)
    }
}

// MARK: - Lifecycle

/// Create a renderer on `ns_view` (an `NSView*`). sugarloaf attaches a
/// `CAMetalLayer` to it. `width`/`height` are physical pixels; `scale` the
/// backing scale factor. `colorspace`: 0 sRGB, 1 DisplayP3, 2 Rec.2020.
/// Returns NULL on a null view or a panic during init.
#[no_mangle]
pub unsafe extern "C" fn sl_new(
    ns_view: *mut c_void,
    width: f32,
    height: f32,
    scale: f32,
    font_size: f32,
    line_height: f32,
    colorspace: u32,
) -> *mut Sl {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(view) = NonNull::new(ns_view) else {
            return std::ptr::null_mut();
        };
        let window = SugarloafWindow {
            handle: RawWindowHandle::AppKit(AppKitWindowHandle::new(view)),
            display: RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            size: SugarloafWindowSize { width, height },
            scale,
        };
        let renderer = SugarloafRenderer {
            backend: default_backend(),
            font_features: None,
            colorspace: colorspace_from_u32(colorspace),
            ..Default::default()
        };
        // The host rasterizes its own glyphs into the atlas, so this font
        // library only backs sugarloaf's (unused-by-us) UI text. A default
        // is enough; `Sugarloaf` holds its own Arc clone, so it can drop.
        let fonts = SugarloafFonts {
            size: font_size,
            ..Default::default()
        };
        let (font_library, _errors) = FontLibrary::new(fonts);
        let layout = RootStyle::new(scale, font_size, line_height);

        // `new` returns the instance even in the error arm (font-not-found);
        // keep it either way.
        let sugarloaf = match Sugarloaf::new(window, renderer, &font_library, layout) {
            Ok(instance) => instance,
            Err(with_errors) => with_errors.instance,
        };
        Box::into_raw(Box::new(Sl {
            inner: sugarloaf,
            rasterizer: rio_grid::GridGlyphRasterizer::new(),
            font_size,
            line_height,
            scale,
            palette: HostPalette::new(),
            had_selection: false,
            had_search: false,
            grid_padding: [0.0; 4],
            padding_extend: 0,
        }))
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn sl_free(sl: *mut Sl) {
    if !sl.is_null() {
        drop(Box::from_raw(sl));
    }
}

// MARK: - Window / config

#[no_mangle]
pub unsafe extern "C" fn sl_resize(sl: *mut Sl, width: u32, height: u32) {
    if let Some(sl) = sl.as_mut() {
        let _ = catch_unwind(AssertUnwindSafe(|| sl.resize(width, height)));
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_rescale(sl: *mut Sl, scale: f32) {
    if let Some(sl) = sl.as_mut() {
        // Keep the stored scale in sync so `sl_render_surface`'s cell
        // metrics track DPI changes.
        sl.scale = scale;
        let _ = catch_unwind(AssertUnwindSafe(|| sl.rescale(scale)));
    }
}

/// Update the font size / line height used to shape glyphs and compute cell
/// metrics. Without this, `sl_render_surface` keeps rendering at the size
/// captured in `sl_new` while the host resizes the grid to the new size —
/// the cells and the grid layout drift apart. The host should resize the
/// grid (via `sl_grid_resize`) after this so the new cols/rows match.
#[no_mangle]
pub unsafe extern "C" fn sl_set_font_size(sl: *mut Sl, font_size: f32, line_height: f32) {
    if let Some(sl) = sl.as_mut() {
        sl.font_size = font_size;
        sl.line_height = line_height;
        // Keep sugarloaf's own layout coherent so a later `rescale` (which
        // recomputes the layout from `RootStyle`) uses the new size.
        let scale = sl.scale;
        let style = sl.style_mut();
        style.font_size = font_size;
        style.line_height = if line_height <= 1.0 { 1.0 } else { line_height };
        let _ = catch_unwind(AssertUnwindSafe(|| sl.rescale(scale)));
    }
}

/// A host font spec. Each family is a UTF-8 C string; a null or empty
/// pointer means "inherit `regular`" for the style fields, and "no override"
/// for `features`. `features` is a comma-separated OpenType feature list
/// (e.g. "calt=0" to disable ligatures, "ss01, cv01" to enable stylistic
/// sets); tag=value or a bare tag (value 1). A `#[repr(C)]` POD by pointer.
#[repr(C)]
pub struct SlFontSpec {
    pub regular: *const c_char,
    pub bold: *const c_char,
    pub italic: *const c_char,
    pub bold_italic: *const c_char,
    pub features: *const c_char,
}

/// A non-empty owned string from a C pointer, or None for null/empty.
unsafe fn cstr_opt(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p)
        .to_str()
        .ok()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Set the fonts used to shape + rasterize the terminal grid: per-style
/// families (bold / italic / bold-italic fall back to `regular` when unset)
/// and an OpenType feature list. Rebuilds the font library, swaps it into
/// sugarloaf, and clears the rasterizer's per-font caches — a font swap
/// reuses the same font ids, so stale glyph resolutions must be dropped. The
/// host MUST also clear the grid's atlas (`sl_grid_clear_atlas`) and force a
/// full rebuild, since the atlas keys glyphs by (font_id, glyph_id) and those
/// now mean new glyphs. A null `spec` restores sugarloaf's default font.
#[no_mangle]
pub unsafe extern "C" fn sl_set_font(sl: *mut Sl, spec: *const SlFontSpec) {
    let Some(sl) = sl.as_mut() else {
        return;
    };
    let (regular, bold, italic, bold_italic, features) = match spec.as_ref() {
        Some(s) => {
            let regular = cstr_opt(s.regular);
            (
                regular.clone(),
                cstr_opt(s.bold).or_else(|| regular.clone()),
                cstr_opt(s.italic).or_else(|| regular.clone()),
                cstr_opt(s.bold_italic).or_else(|| regular.clone()),
                cstr_opt(s.features).map(|f| {
                    f.split(',')
                        .map(|t| t.trim().to_string())
                        .filter(|t| !t.is_empty())
                        .collect::<Vec<_>>()
                }),
            )
        }
        None => (None, None, None, None, None),
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        // Set each style's family explicitly (NOT the top-level `family`,
        // which would overwrite all four with one name) so bold/italic can
        // resolve their own face — or a different family entirely. An unset
        // or unresolvable family falls back inside FontLibrary.
        let mut fonts = SugarloafFonts {
            size: sl.font_size,
            features,
            ..SugarloafFonts::default()
        };
        if let Some(r) = regular {
            fonts.regular.family = r;
        }
        if let Some(b) = bold {
            fonts.bold.family = b;
        }
        if let Some(i) = italic {
            fonts.italic.family = i;
        }
        if let Some(bi) = bold_italic {
            fonts.bold_italic.family = bi;
        }
        let (font_library, _errors) = FontLibrary::new(fonts);
        sl.update_font(&font_library);
        sl.rasterizer.clear_font_caches();
    }));
}

// MARK: - Kitty graphics (images)

/// One image placement on the terminal, in physical pixels. `id` is the kitty
/// image id; `x`/`y`/`w`/`h` the on-screen rect; `z_index` the layer (< 0 under
/// text, >= 0 over it); `src_*` the normalized crop rect (offset + size) into
/// the source image. A `#[repr(C)]` POD.
#[repr(C)]
pub struct SlImageOverlay {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub z_index: i32,
    pub src_x: f32,
    pub src_y: f32,
    pub src_w: f32,
    pub src_h: f32,
}

/// Upload (or replace) an RGBA8 image in the renderer's texture store, keyed by
/// kitty image `id`. `rgba` is `width*height*4` bytes, row-major, no stride.
/// Re-upload only when the image's contents change (keyed by the host's stamp).
#[no_mangle]
pub unsafe extern "C" fn sl_image_upload(
    sl: *mut Sl,
    id: u32,
    width: u32,
    height: u32,
    rgba: *const u8,
    len: usize,
) {
    let Some(sl) = sl.as_mut() else {
        return;
    };
    // Reject a buffer too small for the declared dimensions: the texture
    // upload trusts width*height and would otherwise read past `rgba`.
    let needed = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(4);
    if rgba.is_null() || len == 0 || needed == 0 || len < needed {
        return;
    }
    let pixels = std::slice::from_raw_parts(rgba, needed).to_vec();
    let data = GraphicData {
        id: GraphicId::new(id as u64),
        width: width as usize,
        height: height as usize,
        color_type: ColorType::Rgba,
        pixels,
        is_opaque: false,
        resize: None,
        display_width: None,
        display_height: None,
        transmit_time: std::time::Instant::now(),
    };
    sl.image_data.insert(
        kitty_image_key(id),
        GraphicDataEntry::from_graphic_data(data),
    );
}

/// Drop every uploaded image and placement. Used when the terminal reports no
/// live images (all placements cleared / scrolled away).
#[no_mangle]
pub unsafe extern "C" fn sl_image_evict_all(sl: *mut Sl) {
    if let Some(sl) = sl.as_mut() {
        sl.image_data.clear();
        sl.clear_image_overlays_for(0);
    }
}

/// Replace this frame's image placements. Clears the previous set and pushes
/// `overlays` (physical px). The referenced images must already be uploaded via
/// `sl_image_upload`. Call every frame before `sl_render_surface`.
#[no_mangle]
pub unsafe extern "C" fn sl_image_set_overlays(
    sl: *mut Sl,
    overlays: *const SlImageOverlay,
    len: usize,
) {
    let Some(sl) = sl.as_mut() else {
        return;
    };
    sl.clear_image_overlays_for(0);
    if overlays.is_null() || len == 0 {
        return;
    }
    let slice = std::slice::from_raw_parts(overlays, len);
    for o in slice {
        sl.push_image_overlay(
            0,
            GraphicOverlay {
                image_id: kitty_image_key(o.id),
                x: o.x,
                y: o.y,
                width: o.w,
                height: o.h,
                z_index: o.z_index,
                source_rect: [o.src_x, o.src_y, o.src_x + o.src_w, o.src_y + o.src_h],
            },
        );
    }
}

/// Window background, shown wherever a cell's bg alpha is 0. Components 0..1.
#[no_mangle]
pub unsafe extern "C" fn sl_set_background_color(
    sl: *mut Sl,
    r: f32,
    g: f32,
    b: f32,
    a: f32,
) {
    if let Some(sl) = sl.as_mut() {
        sl.set_background_color(Some(Color {
            r: r as f64,
            g: g as f64,
            b: b as f64,
            a: a as f64,
        }));
    }
}

/// Padding (physical px) between the drawable edges and the cell grid:
/// top, right, bottom, left. `extend` is a bitmask of edges whose
/// row/column background colors extend into the padding band (bit0 left,
/// bit1 right, bit2 up, bit3 down). Applies to `sl_render_surface`; the
/// low-level `sl_render_grid` path takes padding through its uniforms.
#[no_mangle]
pub unsafe extern "C" fn sl_set_grid_padding(
    sl: *mut Sl,
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
    extend: u32,
) {
    if let Some(sl) = sl.as_mut() {
        sl.grid_padding = [top, right, bottom, left];
        sl.padding_extend = extend;
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_set_window_opaque(sl: *mut Sl, opaque: bool) {
    if let Some(sl) = sl.as_ref() {
        sl.set_window_opaque(opaque);
    }
}

/// Integer cell size for a font size / line height / scale, so the host can
/// compute cols = floor(width / cell_width), etc.
#[no_mangle]
pub unsafe extern "C" fn sl_cell_metrics(
    sl: *const Sl,
    font_size: f32,
    line_height: f32,
    scale: f32,
    out: *mut SlCellMetrics,
) {
    let (Some(sl), Some(out)) = (sl.as_ref(), out.as_mut()) else {
        return;
    };
    let (_dims, m) = sl.compute_cell_metrics(font_size, line_height, scale);
    *out = SlCellMetrics {
        cell_width: m.cell_width,
        cell_height: m.cell_height,
        cell_baseline: m.cell_baseline,
    };
}

// MARK: - Grid lifecycle

/// A per-panel cell grid, owning its slice of the atlas + cell buffers.
/// Create against a live `Sl`; free before the `Sl`.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_new(
    sl: *const Sl,
    cols: u32,
    rows: u32,
) -> *mut GridRenderer {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(sl) = sl.as_ref() else {
            return std::ptr::null_mut();
        };
        Box::into_raw(Box::new(GridRenderer::new(&sl.ctx, cols, rows)))
    }))
    .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn sl_grid_free(grid: *mut GridRenderer) {
    if !grid.is_null() {
        drop(Box::from_raw(grid));
    }
}

/// Resize the grid. Discards contents — the host must rewrite every row.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_resize(grid: *mut GridRenderer, cols: u32, rows: u32) {
    if let Some(grid) = grid.as_mut() {
        grid.resize(cols, rows);
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_grid_clear_row(grid: *mut GridRenderer, row: u32) {
    if let Some(grid) = grid.as_mut() {
        grid.clear_row(row);
    }
}

// MARK: - Per-frame content

/// Write one row: `bg` must be exactly `cols` long; `fg` is the glyph +
/// decoration cells (variable length). Both are `#[repr(C)]` POD arrays
/// built by the host.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_write_row(
    grid: *mut GridRenderer,
    row: u32,
    bg: *const CellBg,
    bg_len: usize,
    fg: *const CellText,
    fg_len: usize,
) {
    if let Some(grid) = grid.as_mut() {
        grid.write_row(row, slice_or_empty(bg, bg_len), slice_or_empty(fg, fg_len));
    }
}

/// Cursor sprite cells: `block` for the block cursor, `non_block` for
/// bar/underline/hollow. Either may be empty.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_set_cursor(
    grid: *mut GridRenderer,
    block: *const CellText,
    block_len: usize,
    non_block: *const CellText,
    non_block_len: usize,
) {
    if let Some(grid) = grid.as_mut() {
        grid.set_cursor(
            slice_or_empty(block, block_len),
            slice_or_empty(non_block, non_block_len),
        );
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_grid_needs_full_rebuild(grid: *const GridRenderer) -> bool {
    grid.as_ref()
        .map(|g| g.needs_full_rebuild())
        .unwrap_or(false)
}

#[no_mangle]
pub unsafe extern "C" fn sl_grid_mark_full_rebuild_done(grid: *mut GridRenderer) {
    if let Some(grid) = grid.as_mut() {
        grid.mark_full_rebuild_done();
    }
}

#[no_mangle]
pub unsafe extern "C" fn sl_grid_request_full_rebuild(grid: *mut GridRenderer) {
    if let Some(grid) = grid.as_mut() {
        grid.request_full_rebuild();
    }
}

/// Drop every rasterized glyph in the grid's atlas. Use after a font change:
/// the atlas keys glyphs by (font_id, glyph_id, size_bucket) and a font swap
/// reuses the same ids, so old bitmaps would otherwise alias new glyphs.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_clear_atlas(grid: *mut GridRenderer) {
    if let Some(grid) = grid.as_mut() {
        grid.clear_atlas();
    }
}

// MARK: - Atlas

/// Look up a glyph already in the grayscale atlas. Returns true and fills
/// `out` on a hit.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_lookup_glyph(
    grid: *const GridRenderer,
    font_id: u32,
    glyph_id: u32,
    size_bucket: u16,
    out: *mut SlAtlasSlot,
) -> bool {
    let (Some(grid), Some(out)) = (grid.as_ref(), out.as_mut()) else {
        return false;
    };
    match grid.lookup_glyph(GlyphKey {
        font_id,
        glyph_id,
        size_bucket,
    }) {
        Some(slot) => {
            *out = to_slot(slot);
            true
        }
        None => false,
    }
}

/// Insert an R8 (grayscale) glyph bitmap (`width*height` bytes, no stride)
/// and get back its atlas slot. Returns false if the atlas is full.
#[no_mangle]
pub unsafe extern "C" fn sl_grid_insert_glyph(
    grid: *mut GridRenderer,
    font_id: u32,
    glyph_id: u32,
    size_bucket: u16,
    width: u16,
    height: u16,
    bearing_x: i16,
    bearing_y: i16,
    bytes: *const u8,
    bytes_len: usize,
    out: *mut SlAtlasSlot,
) -> bool {
    let (Some(grid), Some(out)) = (grid.as_mut(), out.as_mut()) else {
        return false;
    };
    let glyph = RasterizedGlyph {
        width,
        height,
        bearing_x,
        bearing_y,
        bytes: slice_or_empty(bytes, bytes_len),
    };
    match grid.insert_glyph(
        GlyphKey {
            font_id,
            glyph_id,
            size_bucket,
        },
        glyph,
    ) {
        Some(slot) => {
            *out = to_slot(slot);
            true
        }
        None => false,
    }
}

fn to_slot(s: sugarloaf::grid::atlas::AtlasSlot) -> SlAtlasSlot {
    SlAtlasSlot {
        x: s.x,
        y: s.y,
        w: s.w,
        h: s.h,
        bearing_x: s.bearing_x,
        bearing_y: s.bearing_y,
        page: s.page,
        _pad: 0,
    }
}

// MARK: - Render

/// Composite one grid + the window background and present. `uniforms`
/// carries the projection, cell/grid size, cursor, and colorspace flags.
#[no_mangle]
pub unsafe extern "C" fn sl_render_grid(
    sl: *mut Sl,
    grid: *mut GridRenderer,
    uniforms: *const GridUniforms,
) {
    let (Some(sl), Some(grid), Some(uniforms)) =
        (sl.as_mut(), grid.as_mut(), uniforms.as_ref())
    else {
        return;
    };
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mut pairs = [(grid, *uniforms)];
        sl.render_with_grids(&mut pairs);
    }));
}

/// Composite N grids in one pass. `grids[i]` pairs with `uniforms[i]`.
/// The grid pointers must be distinct.
#[no_mangle]
pub unsafe extern "C" fn sl_render_grids(
    sl: *mut Sl,
    grids: *const *mut GridRenderer,
    uniforms: *const GridUniforms,
    count: usize,
) {
    let Some(sl) = sl.as_mut() else { return };
    if grids.is_null() || uniforms.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let mut pairs: Vec<(&mut GridRenderer, GridUniforms)> = Vec::with_capacity(count);
        for i in 0..count {
            let grid = *grids.add(i);
            if let Some(grid) = grid.as_mut() {
                pairs.push((grid, *uniforms.add(i)));
            }
        }
        sl.render_with_grids(&mut pairs);
    }));
}

/// Present with no grids (background + any UI overlays only).
#[no_mangle]
pub unsafe extern "C" fn sl_render(sl: *mut Sl) {
    if let Some(sl) = sl.as_mut() {
        let _ = catch_unwind(AssertUnwindSafe(|| sl.render()));
    }
}

/// Drain the frame's queues without presenting (nothing was dirty).
#[no_mangle]
pub unsafe extern "C" fn sl_discard_frame(sl: *mut Sl) {
    if let Some(sl) = sl.as_mut() {
        sl.discard_frame();
    }
}

/// True if the last render dropped its frame (no drawable available, e.g.
/// just after wake); the host should re-mark dirty and try next tick.
#[no_mangle]
pub unsafe extern "C" fn sl_take_frame_dropped(sl: *mut Sl) -> bool {
    sl.as_mut().map(|s| s.take_frame_dropped()).unwrap_or(false)
}

/// Fill `out` (16 floats) with the orthographic projection for a drawable
/// of `width`x`height` physical pixels, for `GridUniforms.projection`.
#[no_mangle]
pub unsafe extern "C" fn sl_orthographic_projection(
    width: f32,
    height: f32,
    out: *mut f32,
) {
    if out.is_null() {
        return;
    }
    let m = sugarloaf::components::core::orthographic_projection(width, height);
    std::ptr::copy_nonoverlapping(m.as_ptr(), out, 16);
}

// MARK: - Terminal surface rendering

/// The `GridPalette` the grid-emit engine (rio-grid) calls into to
/// resolve cell colors. Starts on Rio's default theme (`Colors::default()`)
/// and the derived 256-color `List`; the host overrides it with
/// `sl_set_theme` (see `apply_theme`) so cell/selection/cursor colors track
/// the user's configured scheme. The color-resolution bodies are ported
/// verbatim from rioterm's `Renderer` (`frontends/rioterm/src/renderer/mod.rs`).
struct HostPalette {
    named: Colors,
    list: List,
    draw_bold_text_with_light_colors: bool,
    use_drawable_chars: bool,
    opacity_cells: bool,
    cell_bg_alpha: u8,
    ignore_selection_fg_color: bool,
}

impl HostPalette {
    fn new() -> Self {
        let named = Colors::default();
        let list = List::from(&named);
        Self {
            named,
            list,
            draw_bold_text_with_light_colors: false,
            use_drawable_chars: true,
            opacity_cells: false,
            cell_bg_alpha: 255,
            ignore_selection_fg_color: false,
        }
    }

    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> ColorArray {
        let dim = flags.contains(StyleFlags::DIM);
        let bold = flags.contains(StyleFlags::BOLD);
        match color {
            AnsiColor::Named(ansi) => {
                match (self.draw_bold_text_with_light_colors, dim, bold) {
                    (_, true, true)
                        if ansi == &NamedColor::Foreground
                            && self.named.light_foreground.is_none() =>
                    {
                        self.color(NamedColor::DimForeground as usize, term_colors)
                    }
                    (true, false, true) => {
                        self.color(ansi.to_light() as usize, term_colors)
                    }
                    (_, true, false) | (false, true, true) => {
                        self.color(ansi.to_dim() as usize, term_colors)
                    }
                    _ => self.color(*ansi as usize, term_colors),
                }
            }
            AnsiColor::Spec(rgb) => {
                if !dim {
                    rgb.to_arr()
                } else {
                    rgb.to_arr_with_dim()
                }
            }
            AnsiColor::Indexed(index) => {
                let index = match (dim, index) {
                    (true, 8..=15) => *index as usize - 8,
                    (true, 0..=7) => NamedColor::DimBlack as usize + *index as usize,
                    _ => *index as usize,
                };
                self.color(index, term_colors)
            }
        }
    }

    fn compute_bg_color(
        &self,
        cell_style: &Style,
        term_colors: &TermColors,
    ) -> ColorArray {
        let inverse = cell_style.flags.contains(StyleFlags::INVERSE);
        let dim = inverse && cell_style.flags.contains(StyleFlags::DIM);
        let bold = inverse && cell_style.flags.contains(StyleFlags::BOLD);
        match cell_style.bg {
            AnsiColor::Named(ansi) => {
                let idx = match (self.draw_bold_text_with_light_colors, dim, bold) {
                    (_, true, true)
                        if ansi == NamedColor::Foreground
                            && self.named.light_foreground.is_none() =>
                    {
                        NamedColor::DimForeground as usize
                    }
                    (true, false, true) => ansi.to_light() as usize,
                    (_, true, false) | (false, true, true) => ansi.to_dim() as usize,
                    _ => ansi as usize,
                };
                self.color(idx, term_colors)
            }
            AnsiColor::Spec(rgb) => {
                if dim {
                    (&(rgb * DIM_FACTOR)).into()
                } else {
                    (&rgb).into()
                }
            }
            AnsiColor::Indexed(idx) => {
                let idx = match (self.draw_bold_text_with_light_colors, dim, bold, idx) {
                    (true, false, true, 0..=7) => idx as usize + 8,
                    (false, true, false, 8..=15) => idx as usize - 8,
                    (false, true, false, 0..=7) => {
                        NamedColor::DimBlack as usize + idx as usize
                    }
                    _ => idx as usize,
                };
                self.color(idx, term_colors)
            }
        }
    }

    fn color(&self, idx: usize, term_colors: &TermColors) -> ColorArray {
        term_colors[idx].unwrap_or(self.list[idx])
    }

    /// Replace the base theme with the host's colors, then rederive the
    /// 256-color `List` (dim/bright variants, the color cube, the gray ramp)
    /// from it. Only the colors the host actually configures are overridden;
    /// everything else (search + hint hues, splits) keeps Rio's defaults.
    fn apply_theme(&mut self, theme: &SlTheme) {
        let c = &mut self.named;
        c.black = theme.ansi[0];
        c.red = theme.ansi[1];
        c.green = theme.ansi[2];
        c.yellow = theme.ansi[3];
        c.blue = theme.ansi[4];
        c.magenta = theme.ansi[5];
        c.cyan = theme.ansi[6];
        c.white = theme.ansi[7];
        c.light_black = theme.ansi[8];
        c.light_red = theme.ansi[9];
        c.light_green = theme.ansi[10];
        c.light_yellow = theme.ansi[11];
        c.light_blue = theme.ansi[12];
        c.light_magenta = theme.ansi[13];
        c.light_cyan = theme.ansi[14];
        c.light_white = theme.ansi[15];
        c.foreground = theme.foreground;
        c.cursor = theme.cursor;
        c.vi_cursor = theme.cursor;
        c.selection_background = theme.selection_background;
        c.selection_foreground = theme.selection_foreground;
        let bg = theme.background;
        c.background = (
            bg,
            ColorWGPU {
                r: bg[0] as f64,
                g: bg[1] as f64,
                b: bg[2] as f64,
                a: bg[3] as f64,
            },
        );
        // Dim/light derivations key off the base colors, so any that the host
        // theme didn't set must be recomputed — clear them so `List::from`
        // rederives from the new base rather than keeping stale values.
        c.dim_black = None;
        c.dim_red = None;
        c.dim_green = None;
        c.dim_yellow = None;
        c.dim_blue = None;
        c.dim_magenta = None;
        c.dim_cyan = None;
        c.dim_white = None;
        c.dim_foreground = None;
        // Search highlight colors. Canario's CPU renderer paints translucent
        // overlays (focused #ff9632, others yellow), but CellBg alpha lets the
        // window bg through rather than compositing over the cell — so use
        // opaque equivalents with a dark foreground for legibility. Fixed
        // (not theme-derived): the host's ColorScheme carries no search hue.
        c.search_focused_match_background = [1.0, 0.588, 0.196, 1.0];
        c.search_focused_match_foreground = [0.06, 0.05, 0.05, 1.0];
        c.search_match_background = [0.85, 0.70, 0.20, 1.0];
        c.search_match_foreground = [0.06, 0.05, 0.05, 1.0];
        self.list = List::from(&self.named);
    }
}

impl rio_grid::GridPalette for HostPalette {
    fn named_colors(&self) -> &Colors {
        &self.named
    }
    fn compute_color(
        &self,
        color: &AnsiColor,
        flags: StyleFlags,
        term_colors: &TermColors,
    ) -> ColorArray {
        HostPalette::compute_color(self, color, flags, term_colors)
    }
    fn compute_bg_color(
        &self,
        cell_style: &Style,
        term_colors: &TermColors,
    ) -> ColorArray {
        HostPalette::compute_bg_color(self, cell_style, term_colors)
    }
    fn color(&self, idx: usize, term_colors: &TermColors) -> ColorArray {
        HostPalette::color(self, idx, term_colors)
    }
    fn use_drawable_chars(&self) -> bool {
        self.use_drawable_chars
    }
    fn opacity_cells(&self) -> bool {
        self.opacity_cells
    }
    fn cell_bg_alpha(&self) -> u8 {
        self.cell_bg_alpha
    }
    fn ignore_selection_fg_color(&self) -> bool {
        self.ignore_selection_fg_color
    }
}

/// A host color theme. Each color is RGBA in 0..1 (matching NSColor
/// components). `ansi` is the 16 terminal colors: 0..7 normal, 8..15 bright.
/// The colors not carried here (search + hint hues, splits, and the dim /
/// bright / 256-cube derivations) fall back to Rio's defaults, rederived
/// from these base colors. A `#[repr(C)]` POD passed by pointer.
#[repr(C)]
pub struct SlTheme {
    pub ansi: [[f32; 4]; 16],
    pub foreground: [f32; 4],
    pub background: [f32; 4],
    pub cursor: [f32; 4],
    pub selection_background: [f32; 4],
    pub selection_foreground: [f32; 4],
}

/// Set the color theme the grid-emit path resolves cells against. Takes
/// effect on the next `sl_render_surface`; the host should mark the grid for
/// a full rebuild (or resize) so every row repaints with the new colors.
#[no_mangle]
pub unsafe extern "C" fn sl_set_theme(sl: *mut Sl, theme: *const SlTheme) {
    let (Some(sl), Some(theme)) = (sl.as_mut(), theme.as_ref()) else {
        return;
    };
    sl.palette.apply_theme(theme);
}

/// One search-match highlight span on a visible row. `line` is the viewport
/// row (0 = top of the visible area), `lo`/`hi` inclusive columns, and
/// `focused` marks the currently-selected match (a distinct highlight color).
/// A `#[repr(C)]` POD.
#[repr(C)]
pub struct SlSearchSpan {
    pub line: u16,
    pub lo: u16,
    pub hi: u16,
    pub focused: u8,
}

/// Per-frame inputs to `sl_render_surface` beyond the terminal snapshot: the
/// cursor color + focus, the blink phase, the search-match spans to highlight,
/// and the IME preedit string (or null). A `#[repr(C)]` POD passed by pointer.
#[repr(C)]
pub struct SlSurfaceInputs {
    pub cursor_r: u8,
    pub cursor_g: u8,
    pub cursor_b: u8,
    /// This panel has focus: drives active (block/bar) vs. hollow cursor.
    pub focused: bool,
    /// The cursor should blink (steady when false).
    pub cursor_blinking: bool,
    /// Current blink phase — the lit half when true. Ignored unless blinking.
    pub cursor_blink_visible: bool,
    /// Viewport-relative search-match spans; null / len 0 = none.
    pub search_spans: *const SlSearchSpan,
    pub search_spans_len: usize,
    /// IME preedit text at the cursor (UTF-8 C string), or null when none.
    pub preedit: *const c_char,
}

/// Render one terminal surface into `grid` and present it. `render_state`
/// is an opaque `librio::RenderState*` (the same object the host drives
/// via `rio_render_state_*`); the caller MUST have already refreshed it
/// with `rio_render_state_update` this frame — this does NOT re-snapshot.
///
/// Replicates rioterm's per-frame grid loop: for each dirty row (or every
/// row when the grid needs a full rebuild) it emits `CellBg`/`CellText`
/// via rio-grid's `build_row_bg`/`build_row_fg`, writes the row, builds
/// the cursor sprite + bg-tint uniforms, then composites with
/// `render_with_grids`. `cursor_r/g/b` is the cursor color; `focused`
/// gates the active (block/bar) vs. inactive (hollow) cursor style.
///
/// Resolves cell/selection/cursor colors against the theme set by
/// `sl_set_theme` (Rio's default until the host pushes one). `inputs` carries
/// the cursor color, focus, blink phase, search-match spans, and IME preedit.
#[no_mangle]
pub unsafe extern "C" fn sl_render_surface(
    sl: *mut Sl,
    grid: *mut GridRenderer,
    render_state: *mut c_void,
    inputs: *const SlSurfaceInputs,
) {
    let (Some(sl), Some(grid), Some(inputs)) =
        (sl.as_mut(), grid.as_mut(), inputs.as_ref())
    else {
        return;
    };
    if render_state.is_null() {
        return;
    }
    let rs = &mut *(render_state as *mut librio::RenderState);

    let cursor_r = inputs.cursor_r;
    let cursor_g = inputs.cursor_g;
    let cursor_b = inputs.cursor_b;
    let focused = inputs.focused;
    let cursor_blinking = inputs.cursor_blinking;
    let cursor_blink_visible = inputs.cursor_blink_visible;
    let preedit_str: Option<&str> = if inputs.preedit.is_null() {
        None
    } else {
        CStr::from_ptr(inputs.preedit)
            .to_str()
            .ok()
            .filter(|s| !s.is_empty())
    };
    let preedit_active = preedit_str.is_some();
    let spans: &[SlSearchSpan] =
        if inputs.search_spans.is_null() || inputs.search_spans_len == 0 {
            &[]
        } else {
            std::slice::from_raw_parts(inputs.search_spans, inputs.search_spans_len)
        };

    let _ = catch_unwind(AssertUnwindSafe(move || {
        let font_size = sl.font_size;
        let line_height = sl.line_height;
        let scale = sl.scale;
        // Shaping size in physical pixels (matches rioterm's `font_px`).
        let size_px = font_size * scale;
        let (_dims, metrics) = sl.compute_cell_metrics(font_size, line_height, scale);
        let cell_w = metrics.cell_width as f32;
        let cell_h = metrics.cell_height as f32;
        let font_library = sl.font_library().clone();
        let input_colorspace = sl.input_colorspace();
        let window_size = sl.window_size();

        let palette = &sl.palette;

        let cols = rs.columns();
        let rows = rs.lines();
        let term_colors = *rs.term_colors();
        let (cursor_line, cursor_col) = rs.cursor();
        let cursor_visible = rs.cursor_visible();
        let cursor_shape = rs.cursor_shape();
        let selection = rs.selection();

        // A freshly created / resized grid has zeroed CPU buffers, so refill
        // every row. A live selection or search highlight also forces a full
        // rebuild so the tint tracks changes; the frame after it clears
        // repaints once more (via `had_selection` / `had_search`) to drop the
        // tint from rows the dirty bits won't flag.
        let has_selection = selection.is_some();
        let has_search = !spans.is_empty();
        let needs_rebuild = grid.needs_full_rebuild();
        let full = needs_rebuild
            || has_selection
            || sl.had_selection
            || has_search
            || sl.had_search;
        sl.had_selection = has_selection;
        sl.had_search = has_search;
        if needs_rebuild {
            grid.mark_full_rebuild_done();
        }

        let cols_max = cols.saturating_sub(1) as u16;
        let mut bg_scratch: Vec<CellBg> = Vec::with_capacity(cols);
        let mut fg_scratch: Vec<CellText> = Vec::with_capacity(cols);
        let mut hint_scratch: Vec<rio_grid::RowHint> = Vec::new();

        {
            let rasterizer = &mut sl.rasterizer;
            for y in 0..rows {
                if !full && !rs.row_dirty(y) {
                    continue;
                }
                let Some(row) = rs.rows().get(y) else {
                    continue;
                };
                let row_sel = row_selection_from(selection, y as u16, cols);
                // Per-row search highlights from the viewport-relative spans.
                hint_scratch.clear();
                for s in spans {
                    if s.line as usize == y {
                        let lo = s.lo.min(cols_max);
                        let hi = s.hi.min(cols_max);
                        if lo <= hi {
                            hint_scratch.push(rio_grid::RowHint {
                                lo,
                                hi,
                                tag: if s.focused != 0 {
                                    rio_grid::HintTag::Focused
                                } else {
                                    rio_grid::HintTag::Match
                                },
                            });
                        }
                    }
                }
                rio_grid::build_row_bg(
                    row,
                    cols,
                    rs.row_styles(y),
                    palette,
                    &term_colors,
                    row_sel,
                    &hint_scratch,
                    // IME composition is a frontend concern; the C API
                    // renders no preedit overlay.
                    None,
                    &mut bg_scratch,
                );
                // Break shaping runs around the cursor cell so partial
                // ligature lookahead can't blank pre-cursor cells while
                // the user is typing.
                let cursor_col_for_row = if cursor_visible
                    && y == cursor_line
                    && cursor_shape != librio::CursorShape::Hidden
                {
                    Some(cursor_col as u16)
                } else {
                    None
                };
                rio_grid::build_row_fg(
                    row,
                    cols,
                    y as u16,
                    rs.row_styles(y),
                    rs.extras(),
                    palette,
                    &term_colors,
                    rasterizer,
                    grid,
                    size_px,
                    cell_w,
                    cell_h,
                    row_sel,
                    &hint_scratch,
                    None,
                    &font_library,
                    0,
                    cursor_col_for_row,
                    &mut fg_scratch,
                );
                grid.write_row(y as u32, &bg_scratch, &fg_scratch);
            }
        }

        // Cursor: pick the render style, emit its sprite into the block
        // or tail slot, and (for the block style only) set the bg-tint
        // uniforms so the bg shader paints the cell and the text shader
        // inverts the glyph on top.
        let render_style = rio_grid::cursor_render_style(rio_grid::CursorRenderInputs {
            visible: cursor_visible,
            focused,
            blink_visible: cursor_blink_visible,
            blinking: cursor_blinking,
            preedit: preedit_active,
            shape: cursor_shape,
        });
        let mut block_cursor: Option<CellText> = None;
        let mut tail_cursor: Option<CellText> = None;
        if let Some(style) = render_style {
            let cw = cell_w.round().clamp(1.0, u32::MAX as f32) as u32;
            let ch = cell_h.round().clamp(1.0, u32::MAX as f32) as u32;
            let color = [cursor_r, cursor_g, cursor_b, 255];
            if let Some((is_block, cell)) = rio_grid::cursor_sprite_cell(
                grid,
                style,
                cursor_col as u16,
                cursor_line as u16,
                color,
                cw,
                ch,
            ) {
                if is_block {
                    block_cursor = Some(cell);
                } else {
                    tail_cursor = Some(cell);
                }
            }
        }
        grid.set_cursor(block_cursor.as_slice(), tail_cursor.as_slice());

        let bg_col = palette.named.background.0;
        let fg_col = palette.named.foreground;
        let cursor_color_f = [
            cursor_r as f32 / 255.0,
            cursor_g as f32 / 255.0,
            cursor_b as f32 / 255.0,
            1.0,
        ];
        let (cursor_pos, cursor_col_u, cursor_bg_u) =
            if matches!(render_style, Some(rio_grid::CursorRenderStyle::Block)) {
                (
                    [cursor_col as u32, cursor_line as u32],
                    [bg_col[0], bg_col[1], bg_col[2], bg_col[3]],
                    cursor_color_f,
                )
            } else {
                ([u32::MAX; 2], [0.0; 4], [0.0; 4])
            };

        let uniforms = GridUniforms {
            projection: sugarloaf::components::core::orthographic_projection(
                window_size.width,
                window_size.height,
            ),
            grid_padding: sl.grid_padding,
            cursor_color: cursor_col_u,
            cursor_bg_color: cursor_bg_u,
            cell_size: [cell_w, cell_h],
            grid_size: [cols as u32, rows as u32],
            cursor_pos,
            _pad_cursor: [0; 2],
            min_contrast: 0.0,
            flags: 0,
            padding_extend: sl.padding_extend,
            input_colorspace,
        };

        // IME preedit: draw the composition string in an inverted box at the
        // cursor (theme fg box, bg-colored text, cursor-colored underline) as
        // an immediate-mode overlay on top of the grid — mirrors the CPU
        // renderer's `drawPreedit`. The box isn't in the terminal buffer, so
        // it rides the UI-text pass rather than the cell grid.
        if let Some(preedit) = preedit_str {
            if cursor_visible && cursor_line < rows && cursor_col < cols {
                let scale_f = scale.max(f32::MIN_POSITIVE);
                let cell_w_l = cell_w / scale_f;
                let cell_h_l = cell_h / scale_f;
                let opts = sugarloaf::text::DrawOpts {
                    font_size,
                    color: [
                        (bg_col[0] * 255.0).round().clamp(0.0, 255.0) as u8,
                        (bg_col[1] * 255.0).round().clamp(0.0, 255.0) as u8,
                        (bg_col[2] * 255.0).round().clamp(0.0, 255.0) as u8,
                        255,
                    ],
                    ..Default::default()
                };
                let text_w = sl.text_mut().measure(preedit, &opts).max(cell_w_l);
                let pad_left_l = sl.grid_padding[3] / scale_f;
                let pad_top_l = sl.grid_padding[0] / scale_f;
                let grid_right = pad_left_l + cols as f32 * cell_w_l;
                let mut x = pad_left_l + cursor_col as f32 * cell_w_l;
                if x + text_w > grid_right {
                    x = (grid_right - text_w).max(0.0);
                }
                let y = pad_top_l + cursor_line as f32 * cell_h_l;
                sl.rect(None, x, y, text_w, cell_h_l, fg_col, 0.0, 0);
                let underline_h = (cell_h_l * 0.09).max(1.0);
                sl.rect(
                    None,
                    x,
                    y + cell_h_l - underline_h,
                    text_w,
                    underline_h,
                    cursor_color_f,
                    0.0,
                    0,
                );
                sl.text_mut().draw(x, y, preedit, &opts);
            }
        }

        let mut pairs = [(grid, uniforms)];
        sl.inner.render_with_grids(&mut pairs);
        rs.reset_dirty();
    }));
}
