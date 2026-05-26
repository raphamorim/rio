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
 // 0 → 1
 // | /|
 // 2 → 3
    float2 corner;
    corner.x = float(vid == 1 || vid == 3);
    corner.y = float(vid == 2 || vid == 3);

 // `source_rect` is `[u0, v0, u1, v1]` (origin, end), not (origin, size).
 // `mix(a, b, t)` computes `a + (b-a)*t`, so corner=(0,0) → (u0,v0)
 // and corner=(1,1) → (u1,v1). The previous `xy + zw * corner` form
 // only worked when `xy == [0,0]` (the full-image default).
    float2 tex_coord = mix(instance.source_rect.xy, instance.source_rect.zw, corner);
    float2 image_pos = instance.dest_pos + instance.dest_size * corner;

    ImageVertexOut out;
    out.position = globals.transform * float4(image_pos, 0.0, 1.0);
    out.tex_coord = tex_coord;
    return out;
}

// Mirror ghostty's `image_fragment` (shaders.metal:832-852).
//
// The image atlas is `RGBA8Unorm_sRGB` (see `renderer/mod.rs`), so the
// HW does the sRGB-decode on sample → bilinear filtering happens in
// linear light (otherwise scaled midtones come out gamma-dark). We then
// `unlinearize` back to gamma encoding because the framebuffer is plain
// `BGRA8Unorm` (gamma-space blending). NO sRGB→P3 gamut conversion: the
// drawable is DisplayP3-tagged, so leaving sRGB primaries as-is lets the
// compositor reinterpret them as P3 — technically oversaturating, but
// matching ghostty's punchier emoji / sixel / kitty-graphics look.
//
// Rec.2020 still gets a matrix because its primaries diverge enough from
// P3 that "treat as P3 directly" would clip badly. has no
// Rec.2020 image path, so we own this decision.
static inline float3 linear_to_srgb(float3 c) {
    float3 lo = c * 12.92;
    float3 hi = pow(c, 1.0 / 2.4) * 1.055 - 0.055;
    return select(lo, hi, c > 0.0031308);
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
 // Sample returns linear RGBA (HW sRGB-decoded); alpha is linear by
 // convention, untouched by the format's transfer curve.
    float4 rgba = image_texture.sample(image_sampler, input.tex_coord);
    float3 lin = rgba.rgb;
    if (globals.input_colorspace == 2u) {
        lin = rec2020_to_p3(lin);
    }
    float3 enc = linear_to_srgb(lin);
 // Premultiply alpha (pipeline blend factors are One / OneMinusSrcAlpha).
    return float4(enc * rgba.a, rgba.a);
}
