# libsugarloaf

A C ABI over Rio's [`sugarloaf`](../sugarloaf) GPU renderer, the rendering
counterpart to [`librio`](../librio) (which wraps the terminal core).

Where `librio` gives a host the terminal *state* (parse + VT + a pull API for
cells), `libsugarloaf` gives it the *renderer*: hand it an `NSView`, and
sugarloaf attaches a `CAMetalLayer`, owns the glyph atlas texture, and does
the GPU compositing + present. On macOS this is native Metal (sugarloaf's
default backend), not wgpu.

## Division of labor

sugarloaf does **not** shape or rasterize glyphs. The host owns that (on
macOS, CoreText). Per frame the host:

1. rasterizes each glyph once into an R8 bitmap and inserts it into the
   atlas (`sl_grid_insert_glyph`, cached by `{font_id, glyph_id, size}`),
2. builds flat `sl_cell_bg_t` / `sl_cell_text_t` arrays for the dirty rows
   (`sl_grid_write_row`) with fg/bg colors, selection/search tint and dim
   already resolved,
3. sets the cursor sprites (`sl_grid_set_cursor`),
4. calls `sl_render_grid(sl, grid, uniforms)`.

The POD cell structs are `#[repr(C)]` and mirrored 1:1 in `libsugarloaf.h`,
so the per-frame hot path crosses the ABI as plain pointers, no marshalling.

## Threading

Every call must run on the thread that owns the `NSView` (the main/UI thread
on macOS): `Sugarloaf` and the grids hold GPU handles and are not `Send`.

## Build

`crate-type = ["rlib", "staticlib"]` — links as a static library. The header
and a `SugarloafKit` modulemap live in `include/`. Build against a local Rio
checkout the same way as `librio`.
