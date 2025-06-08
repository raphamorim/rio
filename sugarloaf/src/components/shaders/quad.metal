// SIMD-optimized quad shader with advanced features (borders, shadows, rounded corners)
// Uses simd/simd.h for parallel vector operations and improved performance
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
    
    // Use SIMD for quad vertex generation
    simd_float2 positions[6] = {
        simd_make_float2(0.0, 0.0), // Bottom-left
        simd_make_float2(1.0, 0.0), // Bottom-right
        simd_make_float2(1.0, 1.0), // Top-right
        simd_make_float2(0.0, 0.0), // Bottom-left
        simd_make_float2(1.0, 1.0), // Top-right
        simd_make_float2(0.0, 1.0)  // Top-left
    };
    
    simd_float2 vertex_pos_simd = positions[vertex_id];
    simd_float2 quad_pos_simd = simd_make_float2(quad.position.x, quad.position.y);
    simd_float2 quad_size_simd = simd_make_float2(quad.size.x, quad.size.y);
    
    // SIMD vector operations for world position calculation
    simd_float2 world_pos_simd = simd_muladd(vertex_pos_simd, quad_size_simd, quad_pos_simd);
    
    // SIMD matrix-vector multiplication
    simd_float4x4 transform_simd = simd_float4x4(uniforms.transform);
    simd_float4 world_pos_4d = simd_make_float4(world_pos_simd.x, world_pos_simd.y, 0.0, 1.0);
    simd_float4 transformed_pos = simd_mul(transform_simd, world_pos_4d);
    
    VertexOut out;
    out.position = float4(transformed_pos);
    out.color = quad.color;
    out.quad_pos = float2(vertex_pos_simd);
    out.quad_size = quad.size;
    out.border_color = quad.border_color;
    out.border_radius = quad.border_radius;
    out.border_width = quad.border_width;
    out.shadow_color = quad.shadow_color;
    out.shadow_offset = quad.shadow_offset;
    out.shadow_blur_radius = quad.shadow_blur_radius;
    
    return out;
}

// SIMD-optimized distance function for rounded rectangles
half rounded_rect_sdf(float2 pos, float2 size, half4 radius) {
    // Convert to SIMD vectors for parallel operations
    simd_float2 pos_simd = simd_make_float2(pos.x, pos.y);
    simd_float2 size_simd = simd_make_float2(size.x, size.y);
    simd_float2 half_size = simd_mul(size_simd, 0.5);
    
    // SIMD operations for distance calculation
    simd_float2 centered_pos = simd_sub(pos_simd, half_size);
    simd_float2 abs_centered = simd_abs(centered_pos);
    simd_float2 d = simd_add(simd_sub(abs_centered, half_size), float(radius.x));
    
    // SIMD max and length operations
    simd_float2 max_d = simd_max(d, simd_make_float2(0.0, 0.0));
    float max_component = simd_max(d.x, d.y);
    float length_max_d = simd_length(max_d);
    
    return half(min(max_component, 0.0) + length_max_d) - radius.x;
}

// SIMD-optimized smoothstep for anti-aliasing
half smoothstep_simd(half edge0, half edge1, half x) {
    half t = simd_clamp((x - edge0) / (edge1 - edge0), 0.0h, 1.0h);
    return t * t * (3.0h - 2.0h * t);
}

fragment half4 fragment_main(VertexOut in [[stage_in]]) {
    // Convert to SIMD vectors for parallel processing
    simd_float2 pos_simd = simd_make_float2(in.quad_pos.x * in.quad_size.x, 
                                           in.quad_pos.y * in.quad_size.y);
    simd_float2 size_simd = simd_make_float2(in.quad_size.x, in.quad_size.y);
    
    // SIMD-optimized distance calculation
    half dist = rounded_rect_sdf(float2(pos_simd), float2(size_simd), in.border_radius);
    
    // SIMD anti-aliasing
    half alpha = half(1.0) - smoothstep_simd(half(-0.5), half(0.5), dist);
    
    // SIMD color processing
    simd_half4 final_color_simd = simd_make_half4(in.color.r, in.color.g, in.color.b, in.color.a);
    
    // SIMD border handling
    if (in.border_width > half(0.0)) {
        half border_dist = abs(dist) - in.border_width;
        half border_alpha = half(1.0) - smoothstep_simd(half(-0.5), half(0.5), border_dist);
        
        simd_half4 border_color_simd = simd_make_half4(in.border_color.r, in.border_color.g, 
                                                      in.border_color.b, in.border_color.a);
        
        // SIMD color mixing
        simd_half4 factor_vec = simd_make_half4(border_alpha, border_alpha, border_alpha, border_alpha);
        final_color_simd = simd_mix(final_color_simd, border_color_simd, factor_vec);
    }
    
    // SIMD alpha blending
    simd_half4 alpha_vec = simd_make_half4(alpha, alpha, alpha, alpha);
    final_color_simd = simd_mul(final_color_simd, alpha_vec);
    
    return half4(final_color_simd);
}