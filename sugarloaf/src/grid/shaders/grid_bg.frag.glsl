#version 450

// Per-cell background fragment shader. One sample per framebuffer pixel:
// look up the owning grid cell, apply `padding_extend` clamping at the
// edges, paint the cursor color where applicable, otherwise return the
// stored CellBg colour through the sRGB↔DisplayP3 chain.
//
// Ported 1:1 from `grid_bg_fragment` in
// `sugarloaf/src/grid/shaders/grid.metal`. Field order in `Uniforms`
// must match `GridUniforms` in `sugarloaf/src/grid/cell.rs` and the
// Metal `Uniforms` struct in grid.metal — std140 layout naturally
// matches the hand-packed byte layout used over there.

// `Uniforms` mirrors `GridUniforms` (144B + pad to 160B = 10 × vec4
// blocks under std140). See cell.rs for offsets / sizes; the fields
// here are listed in the same order.
layout(set = 0, binding = 0, std140) uniform Uniforms {
    mat4 projection;        // offset   0
    vec4 grid_padding;      // offset  64  (top, right, bottom, left)
    vec4 cursor_color;      // offset  80
    vec4 cursor_bg_color;   // offset  96
    vec2 cell_size;         // offset 112
    uvec2 grid_size;        // offset 120
    uvec2 cursor_pos;       // offset 128
    uvec2 _pad_cursor;      // offset 136
    float min_contrast;     // offset 144
    uint flags;             // offset 148
    uint padding_extend;    // offset 152
    uint input_colorspace;  // offset 156
} uniforms;

// `CellBg` is 4 bytes (uchar4 rgba) in cell.rs. We expose the storage
// buffer as `uint cells[]` and unpack each entry via
// `unpackUnorm4x8` — that maps byte[0]→x etc., matching the little-
// endian RGBA layout sugarloaf uses on the CPU side.
layout(set = 0, binding = 1, std430) readonly buffer Cells {
    uint cells[];
};

layout(location = 0) out vec4 out_color;

const uint PAD_EXTEND_LEFT  = 1u << 0;
const uint PAD_EXTEND_RIGHT = 1u << 1;
const uint PAD_EXTEND_UP    = 1u << 2;
const uint PAD_EXTEND_DOWN  = 1u << 3;

// ----- colorspace helpers (mirror `grid.metal` 1:1) -----

vec3 grid_srgb_to_linear(vec3 c) {
    vec3 lo = c / 12.92;
    vec3 hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return mix(lo, hi, greaterThan(c, vec3(0.04045)));
}

vec3 grid_linear_to_srgb(vec3 c) {
    vec3 lo = c * 12.92;
    vec3 hi = pow(c, vec3(1.0 / 2.4)) * 1.055 - 0.055;
    return mix(lo, hi, greaterThan(c, vec3(0.0031308)));
}

vec3 grid_srgb_to_p3(vec3 linear_srgb) {
    return vec3(
        dot(linear_srgb, vec3(0.82246197, 0.17753803, 0.0)),
        dot(linear_srgb, vec3(0.03319420, 0.96680580, 0.0)),
        dot(linear_srgb, vec3(0.01708263, 0.07239744, 0.91051993))
    );
}

vec3 grid_rec2020_to_p3(vec3 linear_r2020) {
    return vec3(
        dot(linear_r2020, vec3( 1.34357825, -0.28217967, -0.06139858)),
        dot(linear_r2020, vec3(-0.06529745,  1.08782226, -0.02252481)),
        dot(linear_r2020, vec3( 0.00282179, -0.02598807,  1.02316628))
    );
}

vec3 grid_prepare_output_rgb(vec3 srgb, uint input_colorspace) {
    vec3 lin = grid_srgb_to_linear(srgb);
    if (input_colorspace == 0u) {
        lin = grid_srgb_to_p3(lin);
    } else if (input_colorspace == 2u) {
        lin = grid_rec2020_to_p3(lin);
    }
    return grid_linear_to_srgb(lin);
}

void main() {
    // `gl_FragCoord.xy` is the pixel center in framebuffer pixels.
    // Locate the owning grid cell relative to the grid origin
    // (top-left = grid_padding.w / .x).
    ivec2 orig_grid_pos = ivec2(
        floor((gl_FragCoord.xy - uniforms.grid_padding.wx) / uniforms.cell_size)
    );
    ivec2 grid_pos = orig_grid_pos;

    // Horizontal padding extend / discard.
    if (grid_pos.x < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_LEFT) != 0u) {
            grid_pos.x = 0;
        } else {
            out_color = vec4(0.0);
            return;
        }
    } else if (grid_pos.x > int(uniforms.grid_size.x) - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_RIGHT) != 0u) {
            grid_pos.x = int(uniforms.grid_size.x) - 1;
        } else {
            out_color = vec4(0.0);
            return;
        }
    }

    // Vertical padding extend / discard.
    if (grid_pos.y < 0) {
        if ((uniforms.padding_extend & PAD_EXTEND_UP) != 0u) {
            grid_pos.y = 0;
        } else {
            out_color = vec4(0.0);
            return;
        }
    } else if (grid_pos.y > int(uniforms.grid_size.y) - 1) {
        if ((uniforms.padding_extend & PAD_EXTEND_DOWN) != 0u) {
            grid_pos.y = int(uniforms.grid_size.y) - 1;
        } else {
            out_color = vec4(0.0);
            return;
        }
    }

    // Cursor block fill: only when this fragment's *original* grid_pos
    // (pre-clamp) matches the cursor cell. Bypassing the clamp here
    // keeps the cursor from leaking into the margin on edge rows.
    if (uniforms.cursor_bg_color.a > 0.0
        && orig_grid_pos.x == int(uniforms.cursor_pos.x)
        && orig_grid_pos.y == int(uniforms.cursor_pos.y))
    {
        vec4 c = uniforms.cursor_bg_color;
        c.rgb = grid_prepare_output_rgb(c.rgb, uniforms.input_colorspace);
        c.rgb *= c.a;
        out_color = c;
        return;
    }

    // Load + decode the CellBg.
    uint idx = uint(grid_pos.y) * uniforms.grid_size.x + uint(grid_pos.x);
    vec4 color = unpackUnorm4x8(cells[idx]);
    color.rgb = grid_prepare_output_rgb(color.rgb, uniforms.input_colorspace);
    color.rgb *= color.a;

    out_color = color;
}
