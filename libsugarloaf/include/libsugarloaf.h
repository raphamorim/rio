#ifndef LIBSUGARLOAF_H
#define LIBSUGARLOAF_H

/* libsugarloaf: a C ABI over Rio's sugarloaf GPU renderer.
 *
 * The host provides an NSView; sugarloaf attaches a CAMetalLayer to it and
 * owns the glyph atlas + GPU present. sugarloaf does NOT shape or rasterize:
 * the host rasterizes each glyph once (e.g. via CoreText), inserts the R8
 * bitmap into the atlas (sl_grid_insert_glyph, cached by key), builds the
 * per-row CellBg/CellText arrays, and calls sl_render_grid.
 *
 * Thread rule: every call must run on the thread that owns the NSView (the
 * main/UI thread on macOS). The objects hold GPU handles and are not Send.
 */

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* Opaque handles. */
typedef struct sl_sugarloaf sl_sugarloaf_t;
typedef struct sl_grid sl_grid_t;

/* Colorspace tags for sl_new. */
#define SL_COLORSPACE_SRGB 0u
#define SL_COLORSPACE_DISPLAY_P3 1u
#define SL_COLORSPACE_REC2020 2u

/* CellText.atlas discriminator. */
#define SL_ATLAS_GRAYSCALE 0u
#define SL_ATLAS_COLOR 1u
/* CellText.bools bits. */
#define SL_CELL_NO_MIN_CONTRAST (1u << 0)
#define SL_CELL_IS_CURSOR_GLYPH (1u << 1)
/* GridUniforms.flags bits. */
#define SL_FLAG_DISPLAY_P3 (1u << 0)
#define SL_FLAG_LINEAR_BLENDING (1u << 1)

/* One background cell: premultiplied RGBA8. Alpha 0 = window bg shows
 * through; alpha 255 = explicit fill. Exactly 4 bytes. Mirrors
 * sugarloaf::grid::cell::CellBg. */
typedef struct {
  uint8_t rgba[4];
} sl_cell_bg_t;

/* One glyph/decoration cell. Exactly 32 bytes. Mirrors
 * sugarloaf::grid::cell::CellText field-for-field. */
typedef struct {
  uint32_t glyph_pos[2];  /* atlas x, y */
  uint32_t glyph_size[2]; /* atlas w, h */
  int16_t bearings[2];    /* x-left, y-bottom, px from the cell origin */
  uint16_t grid_pos[2];   /* col, row */
  uint8_t color[4];       /* premultiplied RGBA8 fg (fully resolved) */
  uint8_t atlas;          /* SL_ATLAS_GRAYSCALE | SL_ATLAS_COLOR */
  uint8_t bools;          /* SL_CELL_* bits */
  uint8_t page;           /* atlas page (0 on Metal) */
  uint8_t _pad;
} sl_cell_text_t;

/* A glyph's atlas position + metrics, filled by lookup/insert. */
typedef struct {
  uint16_t x, y, w, h;
  int16_t bearing_x, bearing_y;
  uint8_t page;
  uint8_t _pad;
} sl_atlas_slot_t;

/* Integer cell metrics from sl_cell_metrics. */
typedef struct {
  uint32_t cell_width;
  uint32_t cell_height;
  uint32_t cell_baseline;
} sl_cell_metrics_t;

/* Per-frame, per-grid uniforms. Mirrors sugarloaf::grid::GridUniforms;
 * layout is 16-byte-block aligned. cursor_pos = {UINT32_MAX, UINT32_MAX}
 * means no cursor. */
typedef struct {
  float projection[16];
  float grid_padding[4];   /* top, right, bottom, left */
  float cursor_color[4];   /* glyph swap under the block cursor; a=0 off */
  float cursor_bg_color[4];/* block fill; a=0 off */
  float cell_size[2];      /* physical px */
  uint32_t grid_size[2];   /* cols, rows */
  uint32_t cursor_pos[2];  /* col, row; UINT32_MAX = none */
  uint32_t _pad_cursor[2];
  float min_contrast;      /* WCAG floor; 0 disables */
  uint32_t flags;          /* SL_FLAG_* */
  uint32_t padding_extend; /* bit0 left,1 right,2 up,3 down */
  uint32_t input_colorspace; /* 0 sRGB, 1 P3, 2 Rec.2020 */
} sl_grid_uniforms_t;

/* A host color theme. Each color is RGBA in 0..1. `ansi` is the 16 terminal
 * colors: 0..7 normal, 8..15 bright. Colors not carried here (search + hint
 * hues, splits, and the dim/bright/256-cube derivations) fall back to Rio's
 * defaults, rederived from these base colors. Mirrors capi::SlTheme. */
typedef struct {
  float ansi[16][4];
  float foreground[4];
  float background[4];
  float cursor[4];
  float selection_background[4];
  float selection_foreground[4];
} sl_theme_t;

/* --- Lifecycle --- */

/* Create a renderer on an NSView*. width/height are physical pixels, scale
 * the backing scale factor, colorspace one of SL_COLORSPACE_*. Returns NULL
 * on a null view or init failure. */
sl_sugarloaf_t *sl_new(void *ns_view, float width, float height, float scale,
                       float font_size, float line_height, uint32_t colorspace);
void sl_free(sl_sugarloaf_t *sl);

/* --- Window / config --- */
void sl_resize(sl_sugarloaf_t *sl, uint32_t width, uint32_t height);
void sl_rescale(sl_sugarloaf_t *sl, float scale);
/* Update the font size / line height used to shape glyphs and size cells.
 * Call on a font-size change, before resizing the grid to the new cell size,
 * or the rendered cells drift out of step with the grid layout. */
void sl_set_font_size(sl_sugarloaf_t *sl, float font_size, float line_height);
/* A host font spec. Each family is a UTF-8 C string; NULL or "" means inherit
 * `regular` (for the style fields) or no override (for `features`). `features`
 * is a comma-separated OpenType feature list ("calt=0" to disable ligatures,
 * "ss01, cv01" for stylistic sets); tag=value or a bare tag (value 1).
 * Mirrors capi::SlFontSpec. */
typedef struct {
  const char *regular;
  const char *bold;
  const char *italic;
  const char *bold_italic;
  const char *features;
} sl_font_spec_t;

/* Set the fonts used to shape + rasterize the grid: per-style families and an
 * OpenType feature list. NULL `spec` restores sugarloaf's default. After this,
 * clear the grid atlas (sl_grid_clear_atlas) and force a full rebuild — a font
 * swap reuses glyph ids, so cached cells/atlas entries are stale. */
void sl_set_font(sl_sugarloaf_t *sl, const sl_font_spec_t *spec);

/* --- Kitty graphics (images) --- */

/* One image placement, physical pixels. `id` is the kitty image id; `z_index`
 * layers it (< 0 under text, >= 0 over); `src_*` is the normalized crop rect
 * (offset + size). Mirrors capi::SlImageOverlay. */
typedef struct {
  uint32_t id;
  float x;
  float y;
  float w;
  float h;
  int32_t z_index;
  float src_x;
  float src_y;
  float src_w;
  float src_h;
} sl_image_overlay_t;

/* Upload/replace an RGBA8 image keyed by kitty image `id`. `rgba` is
 * width*height*4 bytes, row-major, no stride. Re-upload only on content
 * change (track the host's per-image stamp). */
void sl_image_upload(sl_sugarloaf_t *sl, uint32_t id, uint32_t width,
                     uint32_t height, const uint8_t *rgba, size_t len);
/* Drop every uploaded image + placement (terminal reports no live images). */
void sl_image_evict_all(sl_sugarloaf_t *sl);
/* Replace this frame's placements (physical px). Referenced images must be
 * uploaded first. Call every frame before sl_render_surface. */
void sl_image_set_overlays(sl_sugarloaf_t *sl, const sl_image_overlay_t *overlays,
                           size_t len);
/* Components 0..1. Shown where a cell's bg alpha is 0. */
void sl_set_background_color(sl_sugarloaf_t *sl, float r, float g, float b, float a);
void sl_set_window_opaque(sl_sugarloaf_t *sl, bool opaque);
/* Set the color theme the terminal grid resolves cells against. Takes effect
 * on the next sl_render_surface; mark the grid for a full rebuild so every
 * row repaints with the new colors. */
void sl_set_theme(sl_sugarloaf_t *sl, const sl_theme_t *theme);
void sl_cell_metrics(const sl_sugarloaf_t *sl, float font_size, float line_height,
                     float scale, sl_cell_metrics_t *out);

/* --- Grid lifecycle --- */
sl_grid_t *sl_grid_new(const sl_sugarloaf_t *sl, uint32_t cols, uint32_t rows);
void sl_grid_free(sl_grid_t *grid);
/* Discards contents; rewrite every row after. */
void sl_grid_resize(sl_grid_t *grid, uint32_t cols, uint32_t rows);
void sl_grid_clear_row(sl_grid_t *grid, uint32_t row);

/* --- Per-frame content --- */
/* bg must be `cols` long; fg is the glyph + decoration cells. */
void sl_grid_write_row(sl_grid_t *grid, uint32_t row, const sl_cell_bg_t *bg,
                       size_t bg_len, const sl_cell_text_t *fg, size_t fg_len);
void sl_grid_set_cursor(sl_grid_t *grid, const sl_cell_text_t *block,
                        size_t block_len, const sl_cell_text_t *non_block,
                        size_t non_block_len);
bool sl_grid_needs_full_rebuild(const sl_grid_t *grid);
void sl_grid_mark_full_rebuild_done(sl_grid_t *grid);
/* Force the next sl_render_surface to repaint every row (e.g. after a theme
 * change, which doesn't dirty any cells). */
void sl_grid_request_full_rebuild(sl_grid_t *grid);
/* Drop every rasterized glyph in the grid's atlas. Use after a font change,
 * where old bitmaps would otherwise alias new glyphs at the same ids. */
void sl_grid_clear_atlas(sl_grid_t *grid);

/* --- Atlas (host rasterizes; sugarloaf packs + uploads) --- */
bool sl_grid_lookup_glyph(const sl_grid_t *grid, uint32_t font_id, uint32_t glyph_id,
                          uint16_t size_bucket, sl_atlas_slot_t *out);
/* R8 bytes, length width*height, no row stride. Returns false if full. */
bool sl_grid_insert_glyph(sl_grid_t *grid, uint32_t font_id, uint32_t glyph_id,
                          uint16_t size_bucket, uint16_t width, uint16_t height,
                          int16_t bearing_x, int16_t bearing_y, const uint8_t *bytes,
                          size_t bytes_len, sl_atlas_slot_t *out);

/* --- Render --- */
void sl_render_grid(sl_sugarloaf_t *sl, sl_grid_t *grid,
                    const sl_grid_uniforms_t *uniforms);
/* grids[i] pairs with uniforms[i]; pointers must be distinct. */
void sl_render_grids(sl_sugarloaf_t *sl, sl_grid_t *const *grids,
                     const sl_grid_uniforms_t *uniforms, size_t count);
void sl_render(sl_sugarloaf_t *sl);
void sl_discard_frame(sl_sugarloaf_t *sl);
bool sl_take_frame_dropped(sl_sugarloaf_t *sl);

/* One search-match highlight span on a visible row. `line` is the viewport
 * row (0 = top of the visible area), `lo`/`hi` inclusive columns, `focused`
 * marks the currently-selected match. Mirrors capi::SlSearchSpan. */
typedef struct {
  uint16_t line;
  uint16_t lo;
  uint16_t hi;
  uint8_t focused;
} sl_search_span_t;

/* Per-frame inputs to sl_render_surface beyond the terminal snapshot.
 * Mirrors capi::SlSurfaceInputs. */
typedef struct {
  uint8_t cursor_r;
  uint8_t cursor_g;
  uint8_t cursor_b;
  bool focused;              /* active (block/bar) vs. hollow cursor */
  bool cursor_blinking;      /* cursor should blink (steady when false) */
  bool cursor_blink_visible; /* current blink phase; lit when true */
  const sl_search_span_t *search_spans; /* viewport-relative; NULL = none */
  size_t search_spans_len;
  const char *preedit;       /* IME preedit at cursor (UTF-8), or NULL */
} sl_surface_inputs_t;

/* Render a librio terminal surface into `grid` and present it in one call:
 * emits CellBg/CellText from the RenderState snapshot (via rio-grid), writes
 * the dirty rows, builds the cursor, applies search highlights, and
 * composites. `render_state` is an opaque librio RenderState* (the pointer the
 * host already holds); it must be refreshed with rio_render_state_update
 * BEFORE this call — this does not re-snapshot. `inputs` carries the cursor
 * color/focus, blink phase, search spans, and IME preedit. */
void sl_render_surface(sl_sugarloaf_t *sl, sl_grid_t *grid, void *render_state,
                       const sl_surface_inputs_t *inputs);

/* out must hold 16 floats. */
void sl_orthographic_projection(float width, float height, float *out);

#ifdef __cplusplus
}
#endif

#endif /* LIBSUGARLOAF_H */
