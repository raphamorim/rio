// Metal quad shader - simplified to match WGPU approach
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

vertex VertexOut vertex_main(uint vertex_id [[vertex_id]],
                           uint instance_id [[instance_id]],
                           constant float2* unit_vertices [[buffer(0)]],
                           constant Quad* quads [[buffer(1)]],
                           constant Uniforms& uniforms [[buffer(2)]]) {

    float2 unit_vertex = unit_vertices[vertex_id];
    constant Quad& quad = quads[instance_id];

    // Simple coordinate calculation like WGPU version
    float2 world_pos = quad.position + unit_vertex * quad.size;
    float4 clip_pos = uniforms.transform * float4(world_pos, 0.0, 1.0);

    VertexOut out;
    out.position = clip_pos;
    out.color = quad.color;
    out.border_color = quad.border_color;
    out.quad_pos = float2(unit_vertex);
    out.quad_size = float2(quad.size);
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
