// Metal quad shader using proper vertex attributes for instancing
#include <metal_stdlib>
#include <simd/simd.h>
using namespace metal;

struct VertexIn {
    // Per-instance attributes from Quad struct
    float4 color [[attribute(0)]];
    float2 position [[attribute(1)]];
    float2 size [[attribute(2)]];
    float4 border_color [[attribute(3)]];
    float4 border_radius [[attribute(4)]];
    float border_width [[attribute(5)]];
    float4 shadow_color [[attribute(6)]];
    float2 shadow_offset [[attribute(7)]];
    float shadow_blur_radius [[attribute(8)]];
};

struct Uniforms {
    float4x4 transform;
    float scale;
};

struct VertexOut {
    float4 position [[position]];
    float4 color;
    float2 quad_pos;
    float2 quad_size;
    float4 border_color;
    float4 border_radius;
    float border_width;
    float4 shadow_color;
    float2 shadow_offset;
    float shadow_blur_radius;
};

// Generate vertex position like WGPU version
float2 vertex_position(uint vertex_index) {
    // Direct translation from WGPU:
    uint2 base = uint2(1, 2) + vertex_index;
    uint2 modulo = base % uint2(6);
    uint2 comparison = select(uint2(0), uint2(1), modulo < uint2(3));
    return float2(comparison);
}

vertex VertexOut vertex_main(uint vertex_id [[vertex_id]],
                           VertexIn input [[stage_in]],
                           constant Uniforms& uniforms [[buffer(1)]]) {

    float2 unit_vertex = vertex_position(vertex_id);

    // Apply scale like WGPU version does
    float2 scaled_position = input.position * uniforms.scale;
    float2 scaled_size = input.size * uniforms.scale;
    
    float2 world_pos = scaled_position + unit_vertex * scaled_size;
    float4 clip_pos = uniforms.transform * float4(world_pos, 0.0, 1.0);

    VertexOut out;
    out.position = clip_pos;
    out.color = input.color;
    out.border_color = input.border_color;
    out.quad_pos = float2(unit_vertex);
    out.quad_size = float2(scaled_size);
    out.border_radius = input.border_radius;
    out.border_width = input.border_width;
    out.shadow_color = input.shadow_color;
    out.shadow_offset = float2(input.shadow_offset);
    out.shadow_blur_radius = input.shadow_blur_radius;

    return out;
}

fragment float4 fragment_main(VertexOut in [[stage_in]]) {
    return in.color;
}