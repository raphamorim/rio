// SIMD-optimized quad shader for Metal backend
// Uses simd/simd.h for parallel vector operations and improved performance
#include <metal_stdlib>
#include <simd/simd.h>
using namespace metal;

struct VertexIn {
    float2 position [[attribute(0)]];
    half4 color [[attribute(1)]];
};

struct VertexOut {
    float4 position [[position]];
    half4 color;
};

struct Uniforms {
    float4x4 transform;
};

vertex VertexOut vertex_main(VertexIn in [[stage_in]],
                           constant Uniforms& uniforms [[buffer(1)]]) {
    VertexOut out;
    
    // SIMD matrix-vector multiplication
    simd_float4x4 transform_simd = simd_float4x4(uniforms.transform);
    simd_float4 position_4d = simd_make_float4(in.position.x, in.position.y, 0.0, 1.0);
    simd_float4 transformed_pos = simd_mul(transform_simd, position_4d);
    
    out.position = float4(transformed_pos);
    out.color = in.color;
    return out;
}

fragment half4 fragment_main(VertexOut in [[stage_in]]) {
    return in.color;
}