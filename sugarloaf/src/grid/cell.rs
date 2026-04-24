// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

//! CPU-side GPU cell structs for the grid renderer.
//!
//! These are written byte-for-byte into the grid's vertex / instance
//! buffers. Both the Metal and wgpu backends reinterpret these slices
//! as raw bytes — `CellBg` and `CellText` are `#[repr(C)]` + bytemuck
//! `Pod` so that's sound.
//!
//! Layout is deliberately identical to Ghostty's `CellBg` / `CellText`
//! (`ghostty/src/renderer/metal/shaders.zig:265-291`) so the shader
//! port in Phase 1 can stay a near-1:1 translation.

use bytemuck::{Pod, Zeroable};

/// Flat per-cell background cell. Indexed by `row * cols + col` in the
/// GPU buffer; the shader reads `bg_cells[instance_id]` where the
/// instance corresponds to a fullscreen grid cell.
///
/// Selection / search / inverse-video tinting is folded into this value
/// on the CPU side before upload — there are no shader-side bits for
/// selection state. Same approach as Ghostty (`generic.zig:2823`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct CellBg {
    /// Premultiplied RGBA8. Alpha = 0 signals "default background"
    /// (window bg shows through); alpha = 255 signals an explicit fill.
    pub rgba: [u8; 4],
}

impl CellBg {
    pub const TRANSPARENT: Self = Self { rgba: [0, 0, 0, 0] };
}

/// Per-glyph instance data. One `CellText` == one textured quad on the
/// GPU. A single terminal cell may emit multiple `CellText`s: one for
/// the base glyph, plus one per decoration (underline / strikethrough
/// / overline / curly underline / hyperlink underline). Matches
/// Ghostty's approach where decorations are separate `CellText`
/// entries rather than bit-packed onto the base glyph.
///
/// Total size is **32 bytes** — verified by the `size_of` const assert
/// below. Keep this stable; the Metal and wgpu vertex-attribute
/// descriptors assume this exact layout and field order.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct CellText {
    /// Atlas coordinates (x, y) of the glyph bitmap's top-left corner.
    pub glyph_pos: [u32; 2],
    /// Glyph bitmap size (w, h) in atlas pixels.
    pub glyph_size: [u32; 2],
    /// Font bearings (x-left, y-bottom), relative to the grid cell
    /// origin, in pixels. `i16` matches Ghostty; if Rio's font metrics
    /// ever exceed ±32k pixels we have bigger problems.
    pub bearings: [i16; 2],
    /// Terminal grid position (col, row). `u16` caps at 65535 which
    /// covers any reasonable terminal size.
    pub grid_pos: [u16; 2],
    /// Premultiplied RGBA8 foreground color (after palette + inverse +
    /// dim + selection/search tint have been applied CPU-side).
    pub color: [u8; 4],
    /// Atlas discriminator:
    ///   0 = grayscale (sampled as alpha mask, multiplied by `color`)
    ///   1 = color (sampled directly, `color` ignored)
    pub atlas: u8,
    /// Packed bits. bit 0 = `no_min_contrast`, bit 1 = `is_cursor_glyph`.
    /// Unused bits reserved for future decoration flags.
    pub bools: u8,
    /// Explicit padding to bring the struct to 32 bytes. Both backends'
    /// vertex descriptors ignore these bytes.
    pub _pad: [u8; 2],
}

impl CellText {
    pub const BOOL_NO_MIN_CONTRAST: u8 = 1 << 0;
    pub const BOOL_IS_CURSOR_GLYPH: u8 = 1 << 1;

    pub const ATLAS_GRAYSCALE: u8 = 0;
    pub const ATLAS_COLOR: u8 = 1;
}

const _: () = {
    // Compile-time size check — downstream shader bindings assume
    // these exact sizes. If you change the struct, update both
    // `grid.metal` and `grid.wgsl` vertex descriptors.
    assert!(std::mem::size_of::<CellBg>() == 4);
    assert!(std::mem::size_of::<CellText>() == 32);
};

/// Per-frame uniform block bound to both bg and text pipelines.
///
/// Field order / sizes mirror Ghostty's `Uniforms` struct
/// (`ghostty/src/renderer/metal/shaders.zig` — search `Uniforms`). The
/// layout is hand-packed so the same bytes round-trip through both
/// Metal (`setVertexBytes`) and wgpu (`write_buffer` into a `uniform`
/// binding). If you add a field:
///
/// - vec4 / mat4x4 members must sit on 16-byte boundaries (WGSL rule).
/// - Group scalars into 16-byte blocks or add explicit `_pad` fields.
/// - Struct size must be a multiple of 16 bytes.
///
/// Current layout: 144 bytes, 16-byte aligned.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct GridUniforms {
    // --- 16-byte-aligned block 1: projection (64 B), offset 0 ---
    /// Orthographic projection (NDC).
    pub projection: [f32; 16],
    // --- 16-byte block: grid_padding (vec4) at offset 64 ---
    /// Padding around the grid inside the drawable (top, right, bottom, left).
    pub grid_padding: [f32; 4],
    // --- 16-byte block: cursor_color (vec4) at offset 80 ---
    /// Cursor foreground swap color (RGBA normalized). Used by the
    /// text fragment shader to override the glyph color for cells
    /// under the cursor — so the character inverts over the cursor
    /// block. Set alpha = 0 to disable.
    pub cursor_color: [f32; 4],
    // --- 16-byte block: cursor_bg_color (vec4) at offset 96 ---
    /// Cursor block fill color (RGBA normalized). Painted by the bg
    /// fragment shader at the actual cursor cell position only —
    /// `padding_extend`'s edge clamping is bypassed for this check
    /// so a cursor on row 0 / row N-1 doesn't bleed into the
    /// window's top/bottom margin. Set alpha = 0 to disable.
    pub cursor_bg_color: [f32; 4],
    // --- 16-byte block of 8-aligned pairs at offset 112 ---
    /// Cell dimensions in physical pixels (w, h).
    pub cell_size: [f32; 2],
    /// Grid size in cells (cols, rows).
    pub grid_size: [u32; 2],
    // --- 16-byte block of 8-aligned pairs at offset 128 ---
    /// Cell coordinates (col, row) of the cursor, for color-swap in the
    /// cell-text fragment shader. When no cursor is active, set to
    /// `[u32::MAX, u32::MAX]`.
    pub cursor_pos: [u32; 2],
    /// Keep the cursor_pos pair 16-aligned with the scalar block below.
    pub _pad_cursor: [u32; 2],
    // --- 16-byte block of scalars at offset 144 ---
    /// Minimum WCAG contrast ratio to enforce against bg. `0.0` disables.
    pub min_contrast: f32,
    /// Bool flags packed into u32 for WGSL compatibility:
    ///   bit 0 = display_p3 colorspace tag
    ///   bit 1 = linear_blending
    pub flags: u32,
    /// Padding-extend bitfield (bit 0 = left, 1 = right, 2 = up, 3 = down).
    pub padding_extend: u32,
    /// How the shader should interpret the sRGB-encoded CPU color
    /// inputs before writing to the DisplayP3-tagged drawable.
    /// - `0` = sRGB      (apply sRGB → DisplayP3 primaries matrix)
    /// - `1` = DisplayP3 (already P3, skip matrix)
    /// - `2` = Rec.2020  (apply Rec.2020 → DisplayP3 matrix)
    ///
    /// Wired to the same source as sugarloaf's rich-text quad
    /// `input_colorspace` (`renderer/mod.rs:264`), so the grid and
    /// every other pipeline agree on the transform. Without this the
    /// grid bg appears brighter/more saturated than the window bg
    /// fill, which runs through `prepare_output_rgb`. Mirrors
    /// Ghostty's `Uniforms.use_display_p3` + `load_color` pair at
    /// `ghostty/src/renderer/shaders/shaders.metal:224`.
    pub input_colorspace: u32,
}

impl GridUniforms {
    pub const FLAG_DISPLAY_P3: u32 = 1 << 0;
    pub const FLAG_LINEAR_BLENDING: u32 = 1 << 1;

    pub const PADDING_EXTEND_LEFT: u32 = 1 << 0;
    pub const PADDING_EXTEND_RIGHT: u32 = 1 << 1;
    pub const PADDING_EXTEND_UP: u32 = 1 << 2;
    pub const PADDING_EXTEND_DOWN: u32 = 1 << 3;
}

const _: () = {
    // Keep the uniform block a multiple of 16 bytes (WGSL / std140).
    assert!(std::mem::size_of::<GridUniforms>() % 16 == 0);
};
