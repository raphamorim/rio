// Metal rich text shader - matching WGPU functionality
#include <metal_stdlib>
#include <simd/simd.h>
using namespace metal;

struct Globals {
    float4x4 transform;
};

struct VertexInput {
    // Per-vertex attributes
    float4 v_pos [[attribute(0)]];
    float4 v_color [[attribute(1)]];
    float2 v_uv [[attribute(2)]];
    int2 layers [[attribute(3)]];
};

struct VertexOutput {
    float4 position [[position]];
    float4 f_color;
    float2 f_uv;
    int color_layer;
    int mask_layer;
};

vertex VertexOutput vs_main(VertexInput input [[stage_in]],
                           constant Globals& globals [[buffer(1)]]) {
    VertexOutput out;
    out.f_color = input.v_color;
    out.f_uv = input.v_uv;
    out.color_layer = input.layers.x;
    out.mask_layer = input.layers.y;
    
    out.position = globals.transform * float4(input.v_pos.x, input.v_pos.y, input.v_pos.z, 1.0);
    return out;
}

fragment float4 fs_main(VertexOutput input [[stage_in]],
                       sampler font_sampler [[sampler(0)]],
                       texture2d<float> color_texture [[texture(0)]],
                       texture2d<float> mask_texture [[texture(1)]]) {
    float4 out = input.f_color;
    
    // For debugging: show all glyphs with mask layer as white
    if (input.mask_layer > 0) {
        out = float4(1.0, 1.0, 1.0, 1.0);
    }
    // Background/cursor rectangles (layers=[0,0]) use vertex color as-is
    
    return out;
}