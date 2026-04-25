#version 450

// Per-instance quad vertex shader, ported from `vs_instanced` in
// `sugarloaf/src/renderer/renderer.metal`. One `QuadInstance` per
// instance, vertex_id picks the corner. 4 vertices per instance,
// drawn as `TRIANGLE_STRIP`.
//
// Per-instance vertex layout matches `QuadInstance` in
// `sugarloaf/src/renderer/batch.rs` (96 bytes):
//   loc 0  R32G32B32_SFLOAT     pos[3]            (offset 0)
//   loc 1  R32G32B32A32_SFLOAT  color[4]          (offset 12)
//   loc 2  R32G32B32A32_SFLOAT  uv_rect[4]        (offset 28)
//   loc 3  R32G32_SINT          layers[2]         (offset 44)
//   loc 4  R32G32_SFLOAT        size[2]           (offset 52)
//   loc 5  R32G32B32A32_SFLOAT  corner_radii[4]   (offset 60)
//   loc 6  R32_SINT             underline_style   (offset 76)
//   loc 7  R32G32B32A32_SFLOAT  clip_rect[4]      (offset 80)
//
// This Vulkan port renders only the SDF rounded-rect fill path; the
// glyph atlas sampling and underline pattern paths from
// renderer.metal are not yet ported (grid text + UI text overlay
// have their own pipelines for those).

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 transform;
    uint input_colorspace;
    uint _pad0;
    uint _pad1;
    uint _pad2;
} globals;

layout(location = 0) in vec3  in_pos;
layout(location = 1) in vec4  in_color;
layout(location = 2) in vec4  in_uv_rect;
layout(location = 3) in ivec2 in_layers;
layout(location = 4) in vec2  in_size;
layout(location = 5) in vec4  in_corner_radii;
layout(location = 6) in int   in_underline_style;
layout(location = 7) in vec4  in_clip_rect;

layout(location = 0)      out vec4 out_color;
layout(location = 1)      out vec2 out_uv;
layout(location = 2) flat out ivec2 out_layers;
layout(location = 3)      out vec4 out_corner_radii;
layout(location = 4)      out vec2 out_rect_size;
layout(location = 5) flat out int  out_underline_style;
layout(location = 6) flat out vec4 out_clip_rect;

// Unit quad corners for triangle strip: TL, BL, TR, BR.
const vec2 UNIT_QUAD[4] = vec2[](
    vec2(0.0, 0.0),
    vec2(0.0, 1.0),
    vec2(1.0, 0.0),
    vec2(1.0, 1.0)
);

void main() {
    vec2 unit = UNIT_QUAD[gl_VertexIndex];
    vec2 pos = in_pos.xy + unit * in_size;
    vec2 uv = mix(in_uv_rect.xy, in_uv_rect.zw, unit);

    gl_Position = globals.transform * vec4(pos, in_pos.z, 1.0);

    out_color = in_color;
    out_uv = uv;
    out_layers = in_layers;
    out_corner_radii = in_corner_radii;
    out_rect_size = in_size;
    out_underline_style = in_underline_style;
    out_clip_rect = in_clip_rect;
}
