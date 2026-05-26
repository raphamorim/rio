#version 450

// Per-vertex non-quad geometry shader, ported from `vs_main` in
// `sugarloaf/src/renderer/renderer.metal`. Used for `polygon()` /
// `line()` / `triangle()` / `arc()` calls — anything that emits
// caller-supplied vertices instead of QuadInstances.
//
// Per-vertex layout matches `Vertex` in
// `sugarloaf/src/renderer/batch.rs` (88 bytes):
//   loc 0  R32G32B32_SFLOAT     pos              (offset 0)
//   loc 1  R32G32B32A32_SFLOAT  color            (offset 12)
//   loc 2  R32G32_SFLOAT        uv               (offset 28)
//   loc 3  R32G32_SINT          layers           (offset 36)
//   loc 4  R32G32B32A32_SFLOAT  corner_radii     (offset 44)
//   loc 5  R32G32_SFLOAT        rect_size        (offset 60)
//   loc 6  R32_SINT             underline_style  (offset 68)
//   loc 7  R32G32B32A32_SFLOAT  clip_rect        (offset 72)
//
// Drawn as TRIANGLE_LIST with one instance — the caller supplies
// every vertex explicitly. Outputs the same `VertexOutput` as
// `quad.vert.glsl` so they share the same fragment shader.

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 transform;
    uint input_colorspace;
    uint _pad0;
    uint _pad1;
    uint _pad2;
} globals;

layout(location = 0) in vec3  in_pos;
layout(location = 1) in vec4  in_color;
layout(location = 2) in vec2  in_uv;
layout(location = 3) in ivec2 in_layers;
layout(location = 4) in vec4  in_corner_radii;
layout(location = 5) in vec2  in_rect_size;
layout(location = 6) in int   in_underline_style;
layout(location = 7) in vec4  in_clip_rect;

layout(location = 0)      out vec4 out_color;
layout(location = 1)      out vec2 out_uv;
layout(location = 2) flat out ivec2 out_layers;
layout(location = 3)      out vec4 out_corner_radii;
layout(location = 4)      out vec2 out_rect_size;
layout(location = 5) flat out int  out_underline_style;
layout(location = 6) flat out vec4 out_clip_rect;

void main() {
    gl_Position = globals.transform * vec4(in_pos, 1.0);
    out_color = in_color;
    out_uv = in_uv;
    out_layers = in_layers;
    out_corner_radii = in_corner_radii;
    out_rect_size = in_rect_size;
    out_underline_style = in_underline_style;
    out_clip_rect = in_clip_rect;
}
