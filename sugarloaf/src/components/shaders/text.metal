// SIMD-optimized text rendering shader
// Uses simd/simd.h for parallel vector operations and improved performance
#include <metal_stdlib>
#include <simd/simd.h>
using namespace metal;

struct VertexIn {
    float2 position [[attribute(0)]];
    float2 tex_coords [[attribute(1)]];
    half4 color [[attribute(2)]];
};

struct VertexOut {
    float4 position [[position]];
    float2 tex_coords;
    half4 color;
};

struct Uniforms {
    float4x4 transform;
};

vertex VertexOut vertex_main(VertexIn in [[stage_in]],
                           constant Uniforms& uniforms [[buffer(1)]]) {
    VertexOut out;
    
    // SIMD matrix-vector multiplication for position transformation
    simd_float4x4 transform_simd = simd_float4x4(uniforms.transform);
    simd_float4 position_4d = simd_make_float4(in.position.x, in.position.y, 0.0, 1.0);
    simd_float4 transformed_pos = simd_mul(transform_simd, position_4d);
    
    out.position = float4(transformed_pos);
    out.tex_coords = in.tex_coords;
    out.color = in.color;
    
    return out;
}

fragment half4 fragment_main(VertexOut in [[stage_in]],
                            texture2d<half> glyph_texture [[texture(0)]],
                            sampler glyph_sampler [[sampler(0)]]) {
    
    // Sample texture alpha
    half alpha = glyph_texture.sample(glyph_sampler, in.tex_coords).r;
    
    // SIMD color processing - multiply color components by alpha in parallel
    simd_half4 color_simd = simd_make_half4(in.color.r, in.color.g, in.color.b, in.color.a);
    simd_half4 alpha_vec = simd_make_half4(1.0h, 1.0h, 1.0h, alpha);
    simd_half4 result = simd_mul(color_simd, alpha_vec);
    
    return half4(result);
}