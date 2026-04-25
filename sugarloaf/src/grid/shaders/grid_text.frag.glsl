#version 450

// Text fragment shader. Two atlases bound — grayscale (R8) for outline
// glyphs, RGBA8 for color emoji. The vertex shader's `out_atlas` flag
// picks which one to read.
//
// Sampling is `texelFetch` (nearest, no filtering) at integer pixel
// coordinates — matches Metal's `coord::pixel` + `filter::nearest`.

layout(set = 1, binding = 0) uniform sampler2D atlas_grayscale;
layout(set = 1, binding = 1) uniform sampler2D atlas_color;

layout(location = 0) flat in uint in_atlas;
layout(location = 1) flat in vec4 in_color;
layout(location = 2)      in vec2 in_tex_coord;

layout(location = 0) out vec4 out_color;

const uint ATLAS_GRAYSCALE = 0u;

void main() {
    ivec2 uv = ivec2(in_tex_coord);
    if (in_atlas == ATLAS_GRAYSCALE) {
        // Grayscale: sample alpha mask, multiply by per-glyph color.
        // Colour is already premultiplied (in_color.rgb *= in_color.a
        // in the vertex shader), so the result is also premultiplied.
        float a = texelFetch(atlas_grayscale, uv, 0).r;
        out_color = in_color * a;
    } else {
        // Color atlas: sample RGBA premultiplied directly.
        out_color = texelFetch(atlas_color, uv, 0);
    }
}
