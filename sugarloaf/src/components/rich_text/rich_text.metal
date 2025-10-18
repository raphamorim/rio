#include <metal_stdlib>
using namespace metal;

// Uniform buffer structure (equivalent to @group(0) @binding(0))
struct Globals {
    float4x4 transform;
};

// Vertex input structure - matches Vertex struct exactly
struct VertexInput {
    float3 v_pos [[attribute(0)]];          // Position (first 12 bytes) - FIXED to float3
    float4 v_color [[attribute(1)]];        // Color (next 16 bytes)
    float2 v_uv [[attribute(2)]];           // UV coords (next 8 bytes)
    int2 layers [[attribute(3)]];           // Layers (next 8 bytes)
    float border_radius [[attribute(4)]];   // Border radius (next 4 bytes)
    float2 rect_size [[attribute(5)]];      // Rect size (next 8 bytes)
};

// Vertex output / Fragment input structure
struct VertexOutput {
    float4 position [[position]];
    float4 f_color;
    float2 f_uv;
    int color_layer;
    int mask_layer;
    float border_radius;
    float2 rect_size;
};

// Vertex shader
vertex VertexOutput vs_main(
    VertexInput input [[stage_in]],
    constant Globals& globals [[buffer(1)]]  // Buffer 1 to match Rust binding
) {
    VertexOutput out;
    out.f_color = input.v_color;
    out.f_uv = input.v_uv;
    out.color_layer = input.layers.x;
    out.mask_layer = input.layers.y;
    out.border_radius = input.border_radius;
    out.rect_size = input.rect_size;

    // Transform position - use float4 constructor with z=0.0, w=1.0
    out.position = globals.transform * float4(input.v_pos.xy, 0.0, 1.0);

    return out;
}

// Signed distance field for rounded rectangle (from Zed's implementation)
float rounded_rect_sdf(float2 corner_center_to_point, float corner_radius) {
    if (corner_radius == 0.0) {
        // Fast path for unrounded corners
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        // Signed distance of the point from a quad that is inset by corner_radius
        // It is negative inside this quad, and positive outside
        float signed_distance_to_inset_quad =
            // 0 inside the inset quad, and positive outside
            length(max(float2(0.0), corner_center_to_point)) +
            // 0 outside the inset quad, and negative inside
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));

        return signed_distance_to_inset_quad - corner_radius;
    }
}

// Fragment shader
fragment float4 fs_main(
    VertexOutput input [[stage_in]],
    texture2d<float> color_texture [[texture(0)]],
    texture2d<float> mask_texture [[texture(1)]],
    sampler font_sampler [[sampler(0)]]
) {
    float4 out = input.f_color;

    if (input.color_layer > 0) {
        out = color_texture.sample(font_sampler, input.f_uv, level(0.0));
    }

    if (input.mask_layer > 0) {
        float mask_alpha = mask_texture.sample(font_sampler, input.f_uv, level(0.0)).x;
        out = float4(out.xyz, input.f_color.a * mask_alpha);
    }

    // Apply rounded rectangle mask if border_radius > 0
    float alpha_factor = 1.0;
    if (input.border_radius > 0.0) {
        float2 half_size = input.rect_size / 2.0;
        // Convert UV (0-1) to local position (-half_size to +half_size)
        float2 local_pos = (input.f_uv - 0.5) * input.rect_size;
        float2 corner_to_point = abs(local_pos) - half_size;
        float2 corner_center_to_point = corner_to_point + input.border_radius;
        float distance = rounded_rect_sdf(corner_center_to_point, input.border_radius);

        // Antialiasing: 0.5 is the threshold for pixel coverage
        alpha_factor = saturate(0.5 - distance);
    }

    return out * float4(1.0, 1.0, 1.0, alpha_factor);
}