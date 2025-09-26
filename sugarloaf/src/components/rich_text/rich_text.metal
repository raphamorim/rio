#include <metal_stdlib>
using namespace metal;

// Uniform buffer structure (equivalent to @group(0) @binding(0))
struct Globals {
    float4x4 transform;
};

// Vertex input structure - matches Vertex struct exactly
struct VertexInput {
    float4 v_pos [[attribute(0)]];      // Position (first 16 bytes)
    float4 v_color [[attribute(1)]];    // Color (next 16 bytes) 
    float2 v_uv [[attribute(2)]];       // UV coords (next 8 bytes)
    int2 layers [[attribute(3)]];       // Layers (next 8 bytes)
};

// Vertex output / Fragment input structure
struct VertexOutput {
    float4 position [[position]];
    float4 f_color;
    float2 f_uv;
    int color_layer;
    int mask_layer;
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
    
    // Transform position - match WGSL exactly:
    // out.position = globals.transform * vec4<f32>(input.v_pos.xy, 0.0, 1.0);
    out.position = globals.transform * float4(input.v_pos.xy, 0.0, 1.0);
    
    return out;
}

// Fragment shader  
fragment float4 fs_main(
    VertexOutput input [[stage_in]],
    texture2d<float> color_texture [[texture(0)]],
    texture2d<float> mask_texture [[texture(1)]],
    sampler font_sampler [[sampler(0)]]
) {
    float4 out = input.f_color;
    
    // Match WGSL logic exactly:
    // if input.color_layer > 0 {
    //     out = textureSampleLevel(color_texture, font_sampler, input.f_uv, 0.0);
    // }
    if (input.color_layer > 0) {
        out = color_texture.sample(font_sampler, input.f_uv, level(0.0));
    }
    
    // if input.mask_layer > 0 {
    //     out = vec4<f32>(out.xyz, input.f_color.a * textureSampleLevel(mask_texture, font_sampler, input.f_uv, 0.0).x);
    // }
    if (input.mask_layer > 0) {
        float mask_alpha = mask_texture.sample(font_sampler, input.f_uv, level(0.0)).x;
        out = float4(out.xyz, input.f_color.a * mask_alpha);
    }
    
    return out;
}
    // Force all pixels to be bright green to test if shader is running at all
 //   return float4(0.0, 1.0, 0.0, 1.0);
