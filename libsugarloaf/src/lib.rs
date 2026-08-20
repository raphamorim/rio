//! libsugarloaf: a C ABI over Rio's `sugarloaf` GPU renderer.
//!
//! Where `librio` wraps the terminal core (parse + VT state + a pull API
//! for cells), libsugarloaf wraps the *renderer*: a host (a Swift/AppKit
//! app) hands it an `NSView`, and sugarloaf owns the `CAMetalLayer`, the
//! glyph atlas texture, and the GPU compositing/present.
//!
//! Division of labor: sugarloaf does NOT shape or rasterize glyphs. The
//! host rasterizes each glyph once (e.g. via CoreText), inserts the R8
//! bitmap into the atlas (`sl_grid_insert_glyph`, cached by key), builds
//! flat `CellBg`/`CellText` arrays per row (`sl_grid_write_row`), sets the
//! cursor, and calls `sl_render_grid`. The POD cell structs are `#[repr(C)]`
//! and mirrored 1:1 in the header, so the per-frame hot path crosses the
//! ABI as plain pointers with no marshalling.

pub mod capi;
