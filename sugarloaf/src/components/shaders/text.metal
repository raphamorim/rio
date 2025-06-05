#include <metal_stdlib>
using namespace metal;

struct VertexIn {
    float2 position [[attribute(0)]];
    float2 tex_coords [[attribute(1)]];
    float4 color [[attribute(2)]];  // Use float4 for compatibility
};

struct VertexOut {
    float4 position [[position]];
    float2 tex_coords;
    float4 color;  // Use float4 for compatibility
};

struct Uniforms {
    float4x4 transform;
};

vertex VertexOut vertex_main(VertexIn in [[stage_in]],
                           constant Uniforms& uniforms [[buffer(1)]]) {
    VertexOut out;
    out.position = uniforms.transform * float4(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    return out;
}

fragment half4 fragment_main(VertexOut in [[stage_in]],
                            texture2d<half> glyph_texture [[texture(0)]],
                            sampler glyph_sampler [[sampler(0)]]) {
    half alpha = glyph_texture.sample(glyph_sampler, in.tex_coords).r;
    return half4(half3(in.color.rgb), half(in.color.a * alpha));
}