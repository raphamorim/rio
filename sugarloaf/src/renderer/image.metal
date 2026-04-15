#include <metal_stdlib>
using namespace metal;

// Image rendering shader — instanced, one instance per image placement.

struct Globals {
    float4x4 transform;
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

// Match `srgb_to_linear` in renderer.metal. Sampled image data is stored as
// sRGB-encoded RGBA (Rgba8Unorm, no HW decode on read) — linearize before
// returning so the `_sRGB` color attachment's on-write encode restores the
// intended pixel value instead of double-encoding it too bright.
static inline float3 srgb_to_linear(float3 c) {
    float3 lo = c / 12.92;
    float3 hi = pow((c + 0.055) / 1.055, 2.4);
    return select(lo, hi, c > 0.04045);
}

fragment float4 image_fs_main(
    ImageVertexOut input [[stage_in]],
    texture2d<float> image_texture [[texture(0)]],
    sampler image_sampler [[sampler(0)]]
) {
    float4 rgba = image_texture.sample(image_sampler, input.tex_coord);
    float3 lin = srgb_to_linear(rgba.rgb);
    // Premultiply alpha
    return float4(lin * rgba.a, rgba.a);
}
