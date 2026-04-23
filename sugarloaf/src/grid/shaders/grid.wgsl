// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

// WGSL grid shader. Peer of `grid.metal`.
//
// Ported from `ghostty/src/renderer/shaders/shaders.metal`:
//   - full_screen_vertex      (line 191 in upstream)
//   - cell_bg_fragment        (line 451)
//
// Phase 1b scope: bg pass only. Same simplifications as the Metal
// port — no full Display P3 / linear-blending chain yet (the colors
// come in already sRGB-encoded from the CPU).
//
// Bindings:
//   @group(0) @binding(0)  Uniforms      (140+4 = 144 bytes)
//   @group(0) @binding(1)  CellBg[]      (cols * rows entries)
//
// Must match `WgpuGridRenderer`'s bind group layout in
// `sugarloaf/src/grid/webgpu.rs`.

struct Uniforms {
    projection:      mat4x4<f32>,   // offset   0
    grid_padding:    vec4<f32>,     //        64
    cursor_color:    vec4<f32>,     //        80
    cursor_bg_color: vec4<f32>,     //        96
    cell_size:       vec2<f32>,     //       112
    grid_size:       vec2<u32>,     //       120
    cursor_pos:      vec2<u32>,     //       128
    _pad_cursor:     vec2<u32>,     //       136
    min_contrast:    f32,           //       144
    flags:           u32,           //       148
    padding_extend:  u32,           //       152
    _pad_tail:       u32,           //       156
};

const PAD_EXTEND_LEFT:  u32 = 1u;
const PAD_EXTEND_RIGHT: u32 = 2u;
const PAD_EXTEND_UP:    u32 = 4u;
const PAD_EXTEND_DOWN:  u32 = 8u;

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

// CellBg is `#[repr(C)] struct { rgba: [u8; 4] }` on the CPU (4 bytes
// each, one u32 when viewed as little-endian). WGSL has no u8, so we
// declare the buffer as `array<u32>` and unpack bytes manually.
// Array length = `cols * rows` (one u32 per cell).
@group(0) @binding(1) var<storage, read> cells: array<u32>;

struct VsOut {
    @builtin(position) position: vec4<f32>,
};

@vertex
fn grid_bg_vertex(@builtin(vertex_index) vid: u32) -> VsOut {
    // Fullscreen triangle (same trick as the Metal port).
    //   vid 0: (-1, -3)
    //   vid 1: (-1,  1)
    //   vid 2: ( 3,  1)
    var x = -1.0;
    var y =  1.0;
    if (vid == 2u) { x =  3.0; }
    if (vid == 0u) { y = -3.0; }

    var out: VsOut;
    out.position = vec4<f32>(x, y, 1.0, 1.0);
    return out;
}

fn load_cell_bg(idx: u32) -> vec4<f32> {
    // One u32 per cell; unpack RGBA little-endian bytes.
    let word = cells[idx];
    let r = f32((word >>  0u) & 0xFFu) / 255.0;
    let g = f32((word >>  8u) & 0xFFu) / 255.0;
    let b = f32((word >> 16u) & 0xFFu) / 255.0;
    let a = f32((word >> 24u) & 0xFFu) / 255.0;
    // Premultiply.
    return vec4<f32>(r * a, g * a, b * a, a);
}

@fragment
fn grid_bg_fragment(in: VsOut) -> @location(0) vec4<f32> {
    // `grid_padding` is (top, right, bottom, left).
    // Use .w (left) + .x (top) to find the grid origin, same as Metal port.
    let cell_fx = (in.position.xy - vec2<f32>(uniforms.grid_padding.w, uniforms.grid_padding.x))
                    / uniforms.cell_size;
    let orig_grid_pos = vec2<i32>(floor(cell_fx));
    var grid_pos = orig_grid_pos;

    // Horizontal padding.
    let cols = i32(uniforms.grid_size.x);
    if (grid_pos.x < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_LEFT) != 0u) {
            grid_pos.x = 0;
        } else {
            return vec4<f32>(0.0);
        }
    } else if (grid_pos.x > cols - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_RIGHT) != 0u) {
            grid_pos.x = cols - 1;
        } else {
            return vec4<f32>(0.0);
        }
    }

    // Vertical padding.
    let rows = i32(uniforms.grid_size.y);
    if (grid_pos.y < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_UP) != 0u) {
            grid_pos.y = 0;
        } else {
            return vec4<f32>(0.0);
        }
    } else if (grid_pos.y > rows - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_DOWN) != 0u) {
            grid_pos.y = rows - 1;
        } else {
            return vec4<f32>(0.0);
        }
    }

    // Cursor overlay at in-bounds cursor cell only (skip
    // padding-extended fragments so an edge cursor doesn't bleed
    // into the window margin).
    if (uniforms.cursor_bg_color.a > 0.0
        && orig_grid_pos.x == i32(uniforms.cursor_pos.x)
        && orig_grid_pos.y == i32(uniforms.cursor_pos.y)) {
        let a = uniforms.cursor_bg_color.a;
        return vec4<f32>(uniforms.cursor_bg_color.rgb * a, a);
    }

    let idx = u32(grid_pos.y) * uniforms.grid_size.x + u32(grid_pos.x);
    return load_cell_bg(idx);
}

// -------------------------------------------------------------------
// Cell Text Shader
//
// WGSL twin of `grid_text_vertex` / `grid_text_fragment` in grid.metal.
// Same simplifications: no full Display P3 / linear-blending chain,
// no min-contrast, single-cell cursor only.
// -------------------------------------------------------------------

const ATLAS_GRAYSCALE: u32 = 0u;
const ATLAS_COLOR:     u32 = 1u;

const BOOL_NO_MIN_CONTRAST: u32 = 1u;
const BOOL_IS_CURSOR_GLYPH: u32 = 2u;

struct CellTextVertexIn {
    // Per-instance attributes (attribute locations match the wgpu
    // vertex buffer layout in grid/webgpu.rs).
    @location(0) glyph_pos:  vec2<u32>,
    @location(1) glyph_size: vec2<u32>,
    @location(2) bearings:   vec2<i32>,
    @location(3) grid_pos:   vec2<u32>,
    @location(4) color:      vec4<f32>,   // UNorm8x4 input, 0..1
    @location(5) atlas:      u32,
    @location(6) bools:      u32,
};

struct TextVsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) atlas: u32,
    @location(1) @interpolate(flat) color: vec4<f32>,
    @location(2) tex_coord: vec2<f32>,
};

// Atlases. Group(1) keeps them separate from the bg bind group so
// the bg pipeline (which doesn't need atlases) uses a smaller bind
// group layout. We use `textureLoad` (no sampler) to match Metal's
// `coord::pixel + filter::nearest` — integer pixel fetch.
@group(1) @binding(0) var atlas_grayscale: texture_2d<f32>;
@group(1) @binding(1) var atlas_color:     texture_2d<f32>;

@vertex
fn grid_text_vertex(
    @builtin(vertex_index) vid: u32,
    in: CellTextVertexIn,
) -> TextVsOut {
    // Cell origin in pixel space.
    let cell_pos = uniforms.cell_size * vec2<f32>(in.grid_pos);

    // Quad corner (0..1) from vertex id — 4-vertex triangle strip.
    //   0 --> 1
    //   |   .'|
    //   |  /  |
    //   | L   |
    //   2 --> 3
    var corner: vec2<f32>;
    corner.x = select(0.0, 1.0, vid == 1u || vid == 3u);
    corner.y = select(0.0, 1.0, vid == 2u || vid == 3u);

    // Glyph bbox inside cell: bearings.x from left, bearings.y from
    // bottom (font convention).
    let size = vec2<f32>(in.glyph_size);
    var offset = vec2<f32>(in.bearings);
    offset.y = uniforms.cell_size.y - offset.y;

    var quad = cell_pos + size * corner + offset;

    // Shift by grid_padding (top/left).
    quad.x += uniforms.grid_padding.w;
    quad.y += uniforms.grid_padding.x;

    var out: TextVsOut;
    out.position = uniforms.projection * vec4<f32>(quad, 0.0, 1.0);

    // Atlas tex coords in PIXEL space — sampler is set to nearest,
    // unnormalized coords equivalent via textureLoad below.
    out.tex_coord = vec2<f32>(in.glyph_pos) + vec2<f32>(in.glyph_size) * corner;
    out.atlas = in.atlas;

    // Foreground color — premultiplied. `in.color` arrives normalized
    // via UNorm8x4 in the vertex buffer layout.
    var color = in.color;
    color = vec4<f32>(color.rgb * color.a, color.a);

    // Cursor-pos fg swap.
    let is_cursor_pos = in.grid_pos.x == uniforms.cursor_pos.x
                     && in.grid_pos.y == uniforms.cursor_pos.y;
    if ((in.bools & BOOL_IS_CURSOR_GLYPH) == 0u && is_cursor_pos) {
        color = uniforms.cursor_color;
        color = vec4<f32>(color.rgb * color.a, color.a);
    }

    out.color = color;
    return out;
}

@fragment
fn grid_text_fragment(in: TextVsOut) -> @location(0) vec4<f32> {
    // Pixel-space tex_coord → integer sample via textureLoad (no
    // sampler filter; matches Metal's `coord::pixel` + `filter::nearest`).
    let ic = vec2<i32>(in.tex_coord);
    if (in.atlas == ATLAS_GRAYSCALE) {
        let a = textureLoad(atlas_grayscale, ic, 0).r;
        return in.color * a;
    } else {
        return textureLoad(atlas_color, ic, 0);
    }
}

