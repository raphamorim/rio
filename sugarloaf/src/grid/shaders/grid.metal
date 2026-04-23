// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

// Metal shader for the grid renderer.
//
// Ported from `ghostty/src/renderer/shaders/shaders.metal`:
//   - full_screen_vertex      (line 191 in upstream)
//   - cell_bg_fragment        (line 451)
//
// Phase 1a scope: bg pass only. `cell_text_*` (text pass) is ported in
// Phase 1c. Color space conversion is deliberately minimal right now:
// the CAMetalLayer in `sugarloaf/src/context/metal.rs` is tagged
// DisplayP3 and `setPresentsWithTransaction:false`, and we emit cell
// colors pre-multiplied in sRGB-gamma space to match the
// non-linear-blending default. The full Ghostty `load_color` chain
// (linearize → sRGB_DP3 → unlinearize) lands when we add the
// `use_display_p3` / `use_linear_blending` uniform paths.

#include <metal_stdlib>

using namespace metal;

// Must match `GridUniforms` in `sugarloaf/src/grid/cell.rs`.
// Field order is load-bearing — vertex descriptor + WGSL port rely on it.
struct Uniforms {
    float4x4 projection;      // offset   0
    float4   grid_padding;    //        64
    float4   cursor_color;    //        80
    float4   cursor_bg_color; //        96
    float2   cell_size;       //       112
    uint2    grid_size;       //       120
    uint2    cursor_pos;      //       128
    uint2    _pad_cursor;     //       136
    float    min_contrast;    //       144
    uint     flags;           //       148
    uint     padding_extend;  //       152
    uint     _pad_tail;       //       156 → total 160
};

constant uint FLAG_DISPLAY_P3      = 1u << 0;
constant uint FLAG_LINEAR_BLENDING = 1u << 1;

constant uint PAD_EXTEND_LEFT  = 1u << 0;
constant uint PAD_EXTEND_RIGHT = 1u << 1;
constant uint PAD_EXTEND_UP    = 1u << 2;
constant uint PAD_EXTEND_DOWN  = 1u << 3;

// Per-cell background. One uchar4 per grid cell, indexed
// `row * grid_size.x + col`. Matches `CellBg` in cell.rs (4 bytes).
// Declared `constant` so Metal places it in constant address space —
// same as Ghostty's `constant uchar4 *cells` parameter at
// `shaders.metal:454`.

//-------------------------------------------------------------------
// Fullscreen vertex — one triangle that covers the viewport.
// Lifted from `full_screen_vertex` (shaders.metal:191).
//-------------------------------------------------------------------
struct FullScreenVertexOut {
    float4 position [[position]];
};

vertex FullScreenVertexOut grid_bg_vertex(uint vid [[vertex_id]]) {
    FullScreenVertexOut out;

    // Single triangle clipped to viewport.
    //   vid 0: (-1, -3)
    //   vid 1: (-1,  1)
    //   vid 2: ( 3,  1)
    float4 position;
    position.x = (vid == 2) ? 3.0 : -1.0;
    position.y = (vid == 0) ? -3.0 :  1.0;
    position.zw = 1.0;
    out.position = position;
    return out;
}

//-------------------------------------------------------------------
// cell_bg fragment — one sample per framebuffer pixel. Looks up the
// owning grid cell, reads its CellBg, applies padding_extend clamping
// at the grid edges, and returns the premultiplied color.
//-------------------------------------------------------------------
fragment float4 grid_bg_fragment(
    FullScreenVertexOut in       [[stage_in]],
    constant Uniforms&   uniforms [[buffer(0)]],
    constant uchar4*     cells    [[buffer(1)]]
) {
    // `in.position.xy` is the pixel's center in framebuffer pixels.
    // `grid_padding` is (top, right, bottom, left) — we only need
    // left (.w) and top (.x) to locate the grid origin.
    int2 orig_grid_pos = int2(
        floor((in.position.xy - uniforms.grid_padding.wx) / uniforms.cell_size)
    );
    int2 grid_pos = orig_grid_pos;

    float4 bg = float4(0.0);

    // Horizontal padding: clamp or discard based on padding_extend bits.
    if (grid_pos.x < 0) {
        if (uniforms.padding_extend & PAD_EXTEND_LEFT) {
            grid_pos.x = 0;
        } else {
            return bg;
        }
    } else if (grid_pos.x > int(uniforms.grid_size.x) - 1) {
        if (uniforms.padding_extend & PAD_EXTEND_RIGHT) {
            grid_pos.x = int(uniforms.grid_size.x) - 1;
        } else {
            return bg;
        }
    }

    // Vertical padding.
    if (grid_pos.y < 0) {
        if (uniforms.padding_extend & PAD_EXTEND_UP) {
            grid_pos.y = 0;
        } else {
            return bg;
        }
    } else if (grid_pos.y > int(uniforms.grid_size.y) - 1) {
        if (uniforms.padding_extend & PAD_EXTEND_DOWN) {
            grid_pos.y = int(uniforms.grid_size.y) - 1;
        } else {
            return bg;
        }
    }

    // Cursor overlay: paint `cursor_bg_color` only when this fragment
    // is inside the actual cursor cell (compare against the original,
    // pre-padding-clamp grid_pos). This keeps the cursor from
    // leaking into the window margin when it sits on an edge row /
    // column and `padding_extend` clamps inward to the cursor cell.
    if (uniforms.cursor_bg_color.a > 0.0
        && orig_grid_pos.x == int(uniforms.cursor_pos.x)
        && orig_grid_pos.y == int(uniforms.cursor_pos.y))
    {
        float4 c = uniforms.cursor_bg_color;
        c.rgb *= c.a;
        return c;
    }

    // Load the cell and convert to normalized premultiplied color.
    uchar4 cell = cells[grid_pos.y * int(uniforms.grid_size.x) + grid_pos.x];
    float4 color = float4(cell) / 255.0;
    color.rgb *= color.a;

    return color;
}

//-------------------------------------------------------------------
// Cell Text Shader
//
// Ported from `ghostty/src/renderer/shaders/shaders.metal:525-761`.
// Phase 1c simplifications:
//   - No Display P3 / linear-blending conversions; colors land
//     already sRGB-encoded.
//   - No WCAG min_contrast enforcement.
//   - No `cursor_wide` handling (single-cell cursor only for now).
//-------------------------------------------------------------------

constant uint ATLAS_GRAYSCALE = 0u;
constant uint ATLAS_COLOR     = 1u;

constant uint BOOL_NO_MIN_CONTRAST  = 1u;
constant uint BOOL_IS_CURSOR_GLYPH  = 2u;

// Per-instance vertex input — mirrors `CellText` in cell.rs.
// `[[attribute]]` indices line up with our Metal vertex descriptor.
struct CellTextVertexIn {
    uint2   glyph_pos  [[attribute(0)]];
    uint2   glyph_size [[attribute(1)]];
    int2    bearings   [[attribute(2)]];
    ushort2 grid_pos   [[attribute(3)]];
    uchar4  color      [[attribute(4)]];
    uchar   atlas      [[attribute(5)]];
    uchar   bools      [[attribute(6)]];
};

struct CellTextVertexOut {
    float4 position  [[position]];
    uint   atlas     [[flat]];
    float4 color     [[flat]];
    float2 tex_coord;
};

//
// Triangle-strip-like quad via a 3-vertex triangle per instance. We use a
// 4-vertex triangle-strip input (vid = 0..3) — same pattern as Ghostty's
// shader to avoid redundant vertex shader invocations.
//
//   0 --> 1
//   |   .'|
//   |  /  |
//   | L   |
//   2 --> 3
//
vertex CellTextVertexOut grid_text_vertex(
    uint                 vid      [[vertex_id]],
    CellTextVertexIn     in       [[stage_in]],
    constant Uniforms&   uniforms [[buffer(1)]]
) {
    // Cell origin in pixel space.
    float2 cell_pos = uniforms.cell_size * float2(in.grid_pos);

    // Quad corner (0..1 in each dim) from vertex id.
    float2 corner;
    corner.x = float(vid == 1 || vid == 3);
    corner.y = float(vid == 2 || vid == 3);

    // Glyph bbox inside the cell: bearings.x from left, bearings.y from
    // bottom (font convention). See Ghostty diagram at shaders.metal:587.
    float2 size   = float2(in.glyph_size);
    float2 offset = float2(in.bearings);
    offset.y = uniforms.cell_size.y - offset.y;

    float2 quad = cell_pos + size * corner + offset;

    // Also shift by grid_padding (top/left) to position the whole grid
    // inside the drawable — `grid_padding` is (top, right, bottom, left).
    quad.x += uniforms.grid_padding.w;
    quad.y += uniforms.grid_padding.x;

    CellTextVertexOut out;
    out.position = uniforms.projection * float4(quad.x, quad.y, 0.0, 1.0);

    // Atlas tex coords in pixel space (the sampler is set to
    // `coord::pixel`, so no normalization needed).
    out.tex_coord = float2(in.glyph_pos) + float2(in.glyph_size) * corner;
    out.atlas = uint(in.atlas);

    // Foreground color — straight u8 → float, premultiplied.
    float4 color = float4(in.color) / 255.0;
    color.rgb *= color.a;

    // Cursor-pos fg swap: if this glyph's cell is under the cursor and
    // it is *not* itself the cursor glyph, use `cursor_color` instead.
    bool is_cursor_pos =
        (uint(in.grid_pos.x) == uniforms.cursor_pos.x) &&
        (uint(in.grid_pos.y) == uniforms.cursor_pos.y);
    if ((in.bools & BOOL_IS_CURSOR_GLYPH) == 0u && is_cursor_pos) {
        color = uniforms.cursor_color;
        color.rgb *= color.a;
    }

    out.color = color;
    return out;
}

fragment float4 grid_text_fragment(
    CellTextVertexOut   in                [[stage_in]],
    texture2d<float>    atlas_grayscale   [[texture(0)]],
    texture2d<float>    atlas_color       [[texture(1)]]
) {
    constexpr sampler atlas_sampler(
        coord::pixel,
        address::clamp_to_edge,
        filter::nearest
    );

    if (in.atlas == ATLAS_GRAYSCALE) {
        // Grayscale atlas: r channel is the alpha mask, multiply by color.
        float a = atlas_grayscale.sample(atlas_sampler, in.tex_coord).r;
        return in.color * a;
    } else {
        // Color atlas: pre-multiplied RGBA directly.
        return atlas_color.sample(atlas_sampler, in.tex_coord);
    }
}

//-------------------------------------------------------------------
// UI text pass. Same atlas + fragment shader as the grid text pass
// (reuses `grid_text_fragment`), but positions glyphs in free pixel
// space instead of on the cell grid. Driven by `sugarloaf::text::Text`;
// overlay call sites (tab titles, search overlay, command palette,
// etc.) queue `TextInstance`s that flush here.
//
// `bearings.x` = distance from `pos.x` to glyph bitmap's left edge.
// `bearings.y` = distance from `pos.y` (text-box top) to glyph bitmap
// top, positive down. See `Text::lookup_or_rasterize_slot` in
// `sugarloaf/src/text.rs` for the conversion from CoreText's
// baseline-relative `top`.
//-------------------------------------------------------------------
struct TextVertexIn {
    float2  pos        [[attribute(0)]];
    uint2   glyph_pos  [[attribute(1)]];
    uint2   glyph_size [[attribute(2)]];
    int2    bearings   [[attribute(3)]];
    uchar4  color      [[attribute(4)]];
    uchar   atlas      [[attribute(5)]];
};

vertex CellTextVertexOut text_vertex(
    uint                vid      [[vertex_id]],
    TextVertexIn        in       [[stage_in]],
    constant float2&    viewport [[buffer(1)]]
) {
    // Quad corner 0..1 in each dim, from vertex id. Matches the
    // triangle-strip-4 pattern in `grid_text_vertex`.
    float2 corner;
    corner.x = float(vid == 1 || vid == 3);
    corner.y = float(vid == 2 || vid == 3);

    float2 size    = float2(in.glyph_size);
    float2 origin  = in.pos + float2(in.bearings);
    float2 quad_px = origin + size * corner;

    // Pixel → NDC (y-flip so `pos.y` grows downward in screen space).
    float2 ndc = float2(
        (quad_px.x / viewport.x) * 2.0 - 1.0,
        1.0 - (quad_px.y / viewport.y) * 2.0
    );

    CellTextVertexOut out;
    out.position  = float4(ndc, 0.0, 1.0);
    out.tex_coord = float2(in.glyph_pos) + size * corner;
    out.atlas     = uint(in.atlas);

    // Premultiplied RGBA. Matches the grid text path's blend model.
    float4 color = float4(in.color) / 255.0;
    color.rgb   *= color.a;
    out.color    = color;
    return out;
}
