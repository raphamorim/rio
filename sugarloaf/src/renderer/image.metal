#include <metal_stdlib>
using namespace metal;

// Image rendering shader — instanced, one instance per image placement.

// Matches the Rust `Globals` in `renderer/mod.rs`. See that file for the
// semantics of `input_colorspace`.
struct Globals {
    float4x4 transform;
    uchar input_colorspace;
};

struct ImageInstanceInput {
    // Screen position of the image top-left (physical pixels).
    float2 dest_pos [[attribute(0)]];
    // Size of the image on screen (physical pixels).
    float2 dest_size [[attribute(1)]];
    // Source rectangle: xy = origin, zw = size (normalized 0..1).
    float4 source_rect [[attribute(2)]];
};

struct ImageVertexOut {
    float4 position [[position]];
    float2 tex_coord;
};

vertex ImageVertexOut image_vs_main(
    uint vid [[vertex_id]],
    ImageInstanceInput instance [[stage_in]],
    constant Globals &globals [[buffer(1)]]
) {
    // Triangle strip: 4 vertices → quad
    //   0 → 1
    //   |  /|
    //   2 → 3
    float2 corner;
    corner.x = float(vid == 1 || vid == 3);
    corner.y = float(vid == 2 || vid == 3);

    float2 tex_coord = instance.source_rect.xy + instance.source_rect.zw * corner;
    float2 image_pos = instance.dest_pos + instance.dest_size * corner;

    ImageVertexOut out;
    out.position = globals.transform * float4(image_pos, 0.0, 1.0);
    out.tex_coord = tex_coord;
    return out;
}

// Match `renderer.metal` — sampled image data is Rgba8Unorm (no HW sRGB
// decode on read) and treated as sRGB-encoded content. Linearize, then
// apply the sRGB → DisplayP3 primaries matrix when the layer is DisplayP3
// with sRGB-interpreted inputs (`input_colorspace == 0`). The framebuffer's
// `_sRGB` on-write encode re-applies the transfer curve after blending.
static inline float3 srgb_to_linear(float3 c) {
    float3 lo = c / 12.92;
    float3 hi = pow((c + 0.055) / 1.055, 2.4);
    return select(lo, hi, c > 0.04045);
}

static inline float3 srgb_to_p3(float3 linear_srgb) {
    return float3(
        dot(linear_srgb, float3(0.82246197, 0.17753803, 0.0)),
        dot(linear_srgb, float3(0.03319420, 0.96680580, 0.0)),
        dot(linear_srgb, float3(0.01708263, 0.07239744, 0.91051993))
    );
}

static inline float3 rec2020_to_p3(float3 linear_r2020) {
    return float3(
        dot(linear_r2020, float3( 1.34357825, -0.28217967, -0.06139858)),
        dot(linear_r2020, float3(-0.06529745,  1.08782226, -0.02252481)),
        dot(linear_r2020, float3( 0.00282179, -0.02598807,  1.02316628))
    );
}

fragment float4 image_fs_main(
    ImageVertexOut input [[stage_in]],
    constant Globals& globals [[buffer(1)]],
    texture2d<float> image_texture [[texture(0)]],
    sampler image_sampler [[sampler(0)]]
) {
    float4 rgba = image_texture.sample(image_sampler, input.tex_coord);
    float3 lin = srgb_to_linear(rgba.rgb);
    if (globals.input_colorspace == 0u) {
        lin = srgb_to_p3(lin);
    } else if (globals.input_colorspace == 2u) {
        lin = rec2020_to_p3(lin);
    }
    // Premultiply alpha
    return float4(lin * rgba.a, rgba.a);
}
