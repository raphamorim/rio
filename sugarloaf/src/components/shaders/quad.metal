#include <metal_stdlib>
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
    float2 quad_pos;
    float2 quad_size;
    half4 border_color;
    half4 border_radius;
    half border_width;
    half4 shadow_color;
    float2 shadow_offset;
    half shadow_blur_radius;
};

struct Uniforms {
    float4x4 transform;
};

vertex VertexOut vertex_main(uint vertex_id [[vertex_id]],
                           uint instance_id [[instance_id]],
                           constant VertexIn* vertices [[buffer(0)]],
                           constant Uniforms& uniforms [[buffer(1)]]) {
    
    constant VertexIn& quad = vertices[instance_id];
    
    // Generate quad vertices (two triangles)
    float2 positions[6] = {
        float2(0.0, 0.0), // Bottom-left
        float2(1.0, 0.0), // Bottom-right
        float2(1.0, 1.0), // Top-right
        float2(0.0, 0.0), // Bottom-left
        float2(1.0, 1.0), // Top-right
        float2(0.0, 1.0)  // Top-left
    };
    
    float2 vertex_pos = positions[vertex_id];
    float2 world_pos = quad.position + vertex_pos * quad.size;
    
    VertexOut out;
    out.position = uniforms.transform * float4(world_pos, 0.0, 1.0);
    out.color = quad.color;
    out.quad_pos = vertex_pos;
    out.quad_size = quad.size;
    out.border_color = quad.border_color;
    out.border_radius = quad.border_radius;
    out.border_width = quad.border_width;
    out.shadow_color = quad.shadow_color;
    out.shadow_offset = quad.shadow_offset;
    out.shadow_blur_radius = quad.shadow_blur_radius;
    
    return out;
}

// Simple distance function for rounded rectangles
half rounded_rect_sdf(float2 pos, float2 size, half4 radius) {
    // Select the appropriate corner radius
    half r = radius.x; // Simplified - use top-left radius for all corners
    
    float2 d = abs(pos - size * 0.5) - size * 0.5 + float(r);
    return half(min(max(d.x, d.y), 0.0) + length(max(d, 0.0))) - r;
}

fragment half4 fragment_main(VertexOut in [[stage_in]]) {
    float2 pos = in.quad_pos * in.quad_size;
    
    // Calculate distance to rounded rectangle
    half dist = rounded_rect_sdf(pos, in.quad_size, in.border_radius);
    
    // Anti-aliasing
    half alpha = half(1.0) - smoothstep(half(-0.5), half(0.5), dist);
    
    // Border handling (simplified)
    half4 final_color = in.color;
    if (in.border_width > half(0.0)) {
        half border_dist = abs(dist) - in.border_width;
        half border_alpha = half(1.0) - smoothstep(half(-0.5), half(0.5), border_dist);
        final_color = mix(in.color, in.border_color, border_alpha);
    }
    
    final_color.a *= alpha;
    return final_color;
}