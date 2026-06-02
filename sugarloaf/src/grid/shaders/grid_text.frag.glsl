#version 450

// Text fragment shader. One combined-image-sampler bound at
// `set=1, binding=0` — the page-list atlas (`grid/vulkan.rs`) binds
// one page's image+sampler here per draw call. The host packs cells
// by (kind, page) into separate buckets, so each draw uniformly
// samples one page, and a fragment-stage push constant carries the
// kind (`is_color`) so the shader picks the right sampling mode.
//
// Sampling is `texelFetch` (nearest, no filtering) at integer pixel
// coordinates — matches Metal's `coord::pixel` + `filter::nearest`.

layout(set = 1, binding = 0) uniform sampler2D atlas;

layout(push_constant) uniform PushConstants {
    // 0 = grayscale (alpha mask × in_color), 1 = color (RGBA premul).
    uint is_color;
} pc;

layout(location = 0) flat in vec4 in_color;
layout(location = 1)      in vec2 in_tex_coord;

layout(location = 0) out vec4 out_color;

void main() {
    ivec2 uv = ivec2(in_tex_coord);
    vec4 s = texelFetch(atlas, uv, 0);
    if (pc.is_color == 0u) {
        // Grayscale: sample alpha mask, multiply by per-glyph color.
        // Color is already premultiplied (in_color.rgb *= in_color.a
        // in the vertex shader), so the result is also premultiplied.
        out_color = in_color * s.r;
    } else {
        // Color atlas: sample RGBA premultiplied directly.
        out_color = s;
    }
}
