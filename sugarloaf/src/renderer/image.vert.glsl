#version 450

// Image vertex shader, ported from `image_vs_main` in
// `sugarloaf/src/renderer/image.metal`. One instance per image
// placement (background image, kitty graphic, sixel image), 4
// vertices per instance drawn as TRIANGLE_STRIP.
//
// Per-instance vertex layout matches `ImageInstance` in
// `sugarloaf/src/renderer/mod.rs` (32 bytes):
//   loc 0  R32G32_SFLOAT       dest_pos     (offset 0)
//   loc 1  R32G32_SFLOAT       dest_size    (offset 8)
//   loc 2  R32G32B32A32_SFLOAT source_rect  (offset 16)

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 transform;
    uint input_colorspace;
    uint _pad0;
    uint _pad1;
    uint _pad2;
} globals;

layout(location = 0) in vec2 in_dest_pos;
layout(location = 1) in vec2 in_dest_size;
layout(location = 2) in vec4 in_source_rect;

layout(location = 0) out vec2 out_tex_coord;

void main() {
    // Triangle strip: 4 vertices → quad
    //   0 → 1
    //   |  /|
    //   2 → 3
    vec2 corner;
    corner.x = float(gl_VertexIndex == 1 || gl_VertexIndex == 3);
    corner.y = float(gl_VertexIndex == 2 || gl_VertexIndex == 3);

    // `source_rect` is `[u0, v0, u1, v1]` (origin, end).
    out_tex_coord = mix(in_source_rect.xy, in_source_rect.zw, corner);

    vec2 image_pos = in_dest_pos + in_dest_size * corner;
    gl_Position = globals.transform * vec4(image_pos, 0.0, 1.0);
}
