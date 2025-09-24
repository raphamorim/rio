// Metal quad shader with proper SIMD functions
#include <metal_stdlib>
#include <simd/simd.h>
using namespace metal;

struct VertexIn {
    half4 color [[attribute(0)]];
    float2 position [[attribute(1)]];
    float2 size [[attribute(2)]];
    half4 border_color [[attribute(3)]];
    half4 border_radius [[attribute(4)]];
    half border_width [[attribute(5)]];
    half4 shadow_color [[attribute(6)]];
    float2 shadow_offset [[attribute(7)]];
    half shadow_blur_radius [[attribute(8)]];
};

struct VertexOut {
    float4 position [[position]];
    half4 color;
    half2 quad_pos;
    half2 quad_size;
    half4 border_color;
    half4 border_radius;
    half border_width;
    half4 shadow_color;
    half2 shadow_offset;
    half shadow_blur_radius;
};

struct Uniforms {
    float4x4 transform;
    float scale;
};

vertex VertexOut vertex_main(uint vertex_id [[vertex_id]],
                           uint instance_id [[instance_id]],
                           constant VertexIn* vertices [[buffer(0)]],
                           constant Uniforms& uniforms [[buffer(1)]]) {

    constant VertexIn& input = vertices[instance_id];

    // Simple quad positions
    float2 positions[6] = {
        float2(0.0, 0.0), // Bottom-left
        float2(1.0, 0.0), // Bottom-right
        float2(1.0, 1.0), // Top-right
        float2(0.0, 0.0), // Bottom-left
        float2(1.0, 1.0), // Top-right
        float2(0.0, 1.0)  // Top-left
    };

    float2 vertex_pos = positions[vertex_id];
    float2 world_pos = input.position + vertex_pos * input.size;
    float4 clip_pos = uniforms.transform * float4(world_pos, 0.0, 1.0);

    VertexOut out;
    out.position = clip_pos;
    out.color = input.color;
    out.border_color = input.border_color;
    out.quad_pos = half2(vertex_pos);
    out.quad_size = half2(input.size);
    out.border_radius = input.border_radius;
    out.border_width = input.border_width;
    out.shadow_color = input.shadow_color;
    out.shadow_offset = half2(input.shadow_offset);
    out.shadow_blur_radius = input.shadow_blur_radius;

    return out;
}

fragment half4 fragment_main(VertexOut in [[stage_in]]) {
    return in.color;
}
