// Metal quad shader - matching WGPU approach with generated vertices
#include <metal_stdlib>
#include <simd/simd.h>
using namespace metal;

struct Quad {
    float4 color;
    float2 position;
    float2 size;
    float4 border_color;
    float4 border_radius;
    float border_width;
    float4 shadow_color;
    float2 shadow_offset;
    float shadow_blur_radius;
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
    // Same logic as WGPU: generate unit quad positions
    // #: 0 1 2 3 4 5
    // x: 1 1 0 0 0 1  
    // y: 1 0 0 0 1 1
    uint2 temp = (uint2(1, 2) + vertex_index) % uint2(6);
    uint2 result = select(uint2(0), uint2(1), temp < uint2(3));
    return float2(result);
}

vertex VertexOut vertex_main(uint vertex_id [[vertex_id]],
                           uint instance_id [[instance_id]],
                           constant Quad* quads [[buffer(0)]],
                           constant Uniforms& uniforms [[buffer(1)]]) {

    float2 unit_vertex = vertex_position(vertex_id);
    constant Quad& quad = quads[instance_id];

    // Apply scale like WGPU version does
    float2 scaled_position = quad.position * uniforms.scale;
    float2 scaled_size = quad.size * uniforms.scale;
    
    float2 world_pos = scaled_position + unit_vertex * scaled_size;
    float4 clip_pos = uniforms.transform * float4(world_pos, 0.0, 1.0);

    VertexOut out;
    out.position = clip_pos;
    out.color = quad.color;
    out.border_color = quad.border_color;
    out.quad_pos = float2(unit_vertex);
    out.quad_size = float2(scaled_size);
    out.border_radius = quad.border_radius;
    out.border_width = quad.border_width;
    out.shadow_color = quad.shadow_color;
    out.shadow_offset = float2(quad.shadow_offset);
    out.shadow_blur_radius = quad.shadow_blur_radius;

    return out;
}

fragment float4 fragment_main(VertexOut in [[stage_in]]) {
    return in.color;
}
