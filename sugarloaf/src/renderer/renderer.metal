#include <metal_stdlib>
using namespace metal;

// Uniform buffer structure (equivalent to @group(0) @binding(0)).
//
// Matches the Rust `Globals` in `renderer/mod.rs` — `input_colorspace` is a
// u8 followed by 15 bytes of padding so the struct is 16-byte aligned.
// 0 = sRGB (apply sRGB → DisplayP3 matrix after linearize), 1 = DisplayP3
// (no matrix), 2 = Rec.2020 (no matrix; real conversion TBD).
struct Globals {
    float4x4 transform;
    uchar input_colorspace;
};

// QuadInstance - matches Rust QuadInstance struct (96 bytes, per-instance)
// All packed types to match C/Rust layout without Metal alignment padding.
struct QuadInstance {
    packed_float3 pos;          // 12 bytes, offset 0
    packed_float4 color;        // 16 bytes, offset 12
    packed_float4 uv_rect;     // 16 bytes, offset 28
    packed_int2 layers;         // 8 bytes, offset 44
    packed_float2 size;         // 8 bytes, offset 52
    packed_float4 corner_radii; // 16 bytes, offset 60
    int underline_style;        // 4 bytes, offset 76
    packed_float4 clip_rect;   // 16 bytes, offset 80
};

// Unit quad corners for triangle strip (TL, BL, TR, BR)
constant float2 UNIT_QUAD[4] = {
    float2(0.0, 0.0),
    float2(0.0, 1.0),
    float2(1.0, 0.0),
    float2(1.0, 1.0),
};

// Vertex input structure - matches Vertex struct (for lines/triangles/arcs)
struct VertexInput {
    float3 v_pos [[attribute(0)]];          // Position (12 bytes)
    float4 v_color [[attribute(1)]];        // Background color / underline color (16 bytes)
    float2 v_uv [[attribute(2)]];           // UV coords (8 bytes)
    int2 layers [[attribute(3)]];           // Layers (8 bytes)
    float4 corner_radii [[attribute(4)]];   // Corner radii / for underlines: [thickness, 0, 0, 0] (16 bytes)
    float2 rect_size [[attribute(5)]];      // Rect size / underline [width, height] (8 bytes)
    int underline_style [[attribute(6)]];   // 0 = none, 1 = regular, 2 = dashed, 3 = dotted, 4 = curly (4 bytes)
    float4 clip_rect [[attribute(7)]];      // [x, y, width, height] in pixels (16 bytes)
};

// Vertex output / Fragment input structure
struct VertexOutput {
    float4 position [[position]];
    float4 f_color;
    float2 f_uv;
    int color_layer;
    int mask_layer;
    float4 corner_radii;
    float2 rect_size;
    int underline_style [[flat]];
    float4 clip_rect [[flat]];
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
    out.corner_radii = input.corner_radii;
    out.rect_size = input.rect_size;
    out.underline_style = input.underline_style;
    out.clip_rect = input.clip_rect;

    out.position = globals.transform * float4(input.v_pos, 1.0);

    return out;
}

// Instanced vertex shader — one QuadInstance per quad, vertex_id picks corner
vertex VertexOutput vs_instanced(
    uint vertex_id [[vertex_id]],
    uint instance_id [[instance_id]],
    const device QuadInstance* instances [[buffer(0)]],
    constant Globals& globals [[buffer(1)]]
) {
    const device QuadInstance& inst = instances[instance_id];
    float2 sz = float2(inst.size);
    float4 uv_r = float4(inst.uv_rect);
    float2 unit = UNIT_QUAD[vertex_id];
    float2 pos = float2(inst.pos[0], inst.pos[1]) + unit * sz;
    float2 uv = mix(uv_r.xy, uv_r.zw, unit);

    VertexOutput out;
    out.position = globals.transform * float4(pos, inst.pos[2], 1.0);
    out.f_color = float4(inst.color);
    out.f_uv = uv;
    out.color_layer = int2(inst.layers).x;
    out.mask_layer = int2(inst.layers).y;
    out.corner_radii = float4(inst.corner_radii);
    out.rect_size = sz;
    out.underline_style = inst.underline_style;
    out.clip_rect = float4(inst.clip_rect);
    return out;
}

// sRGB → linear light. Colors uploaded from the CPU side are sRGB-encoded
// 8-bit values normalized to 0..1. The color attachment's `_sRGB` pixel
// format expects the fragment to emit *linear* RGB: the GPU then applies
// the sRGB transfer curve on write and its inverse on read so alpha
// blending runs in linear light. Without this the HW gamma encode would
// shift every pixel too bright and defeat the point of the _sRGB target.
// Alpha is already linear by convention — do not transform it.
float3 srgb_to_linear(float3 c) {
    float3 lo = c / 12.92;
    float3 hi = pow((c + 0.055) / 1.055, 2.4);
    return select(lo, hi, c > 0.04045);
}

// Bradford-adapted sRGB D65 primaries → DisplayP3 D65 primaries, in linear
// light. Required because our CAMetalLayer is tagged DisplayP3: a linear
// value we write is interpreted with P3's primaries, so we must convert
// input colors to match. Applied only when `input_colorspace == 0` (sRGB) —
// skipping it leaves `#ff0000` displaying as P3-pure red (oversaturated
// vs. the sRGB standard red every other app draws). Matches ghostty's
// `kSrgbToDisplayP3` / the colour.science constant.
float3 srgb_to_p3(float3 linear_srgb) {
    return float3(
        dot(linear_srgb, float3(0.82246197, 0.17753803, 0.0)),
        dot(linear_srgb, float3(0.03319420, 0.96680580, 0.0)),
        dot(linear_srgb, float3(0.01708263, 0.07239744, 0.91051993))
    );
}

// One-shot: sRGB-encoded → linear, then (if `input_colorspace == 0`) to
// DisplayP3 primaries. Every fragment return path goes through this so the
// framebuffer — which is sRGB-transfer-curve encoded but DisplayP3-tagged —
// shows the intended colour.
float3 prepare_output_rgb(float3 srgb, uchar input_colorspace) {
    float3 lin = srgb_to_linear(srgb);
    if (input_colorspace == 0u) {
        lin = srgb_to_p3(lin);
    }
    return lin;
}

// Pick the corner radius based on which quadrant the point is in
float pick_corner_radius(float2 center_to_point, float4 corner_radii) {
    if (center_to_point.x < 0.0) {
        if (center_to_point.y < 0.0) {
            return corner_radii.x; // top_left
        } else {
            return corner_radii.w; // bottom_left
        }
    } else {
        if (center_to_point.y < 0.0) {
            return corner_radii.y; // top_right
        } else {
            return corner_radii.z; // bottom_right
        }
    }
}

// Signed distance field for a quad (rectangle)
float quad_sdf(float2 corner_center_to_point, float corner_radius) {
    if (corner_radius == 0.0) {
        // Fast path for sharp corners
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        // Signed distance of the point from a quad that is inset by corner_radius.
        // It is negative inside this quad, and positive outside.
        float signed_distance_to_inset_quad =
            // 0 inside the inset quad, and positive outside.
            length(max(float2(0.0), corner_center_to_point)) +
            // 0 outside the inset quad, and negative inside.
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));
        return signed_distance_to_inset_quad - corner_radius;
    }
}

constant float PI_F = 3.1415926;

// Modulus that has the same sign as `a`.
float fmod_pos(float a, float b) {
    return a - b * trunc(a / b);
}

// Calculate underline alpha for pattern rendering
float underline_alpha(float x_pos, float y_pos, float rect_height, float thickness, int style) {
    // style 1: regular solid line
    if (style == 1) {
        return 1.0;
    }

    // style 2: dashed (6px dash, 2px gap)
    if (style == 2) {
        float antialias = 0.5;
        float dash_width = 6.0;
        float gap_width = 2.0;
        float period = dash_width + gap_width;
        float pos_in_period = fmod_pos(x_pos, period);
        float start_aa = saturate(pos_in_period / antialias);
        float end_aa = saturate((dash_width - pos_in_period) / antialias);
        return min(start_aa, end_aa);
    }

    // style 3: dotted (2px dot, 2px gap)
    if (style == 3) {
        float antialias = 0.5;
        float dot_width = 2.0;
        float gap_width = 2.0;
        float period = dot_width + gap_width;
        float pos_in_period = fmod_pos(x_pos, period);
        float start_aa = saturate(pos_in_period / antialias);
        float end_aa = saturate((dot_width - pos_in_period) / antialias);
        return min(start_aa, end_aa);
    }

    // style 4: curly (sine wave) using SDF
    if (style == 4) {
        const float WAVE_FREQUENCY = 2.0;
        const float WAVE_HEIGHT_RATIO = 0.8;

        float half_thickness = thickness * 0.5;
        float2 st = float2(x_pos / rect_height, y_pos / rect_height - 0.5);
        float frequency = PI_F * WAVE_FREQUENCY * thickness / rect_height;
        float amplitude = (thickness * WAVE_HEIGHT_RATIO) / rect_height;

        float sine = sin(st.x * frequency) * amplitude;
        float dSine = cos(st.x * frequency) * amplitude * frequency;
        float dist = (st.y - sine) / sqrt(1.0 + dSine * dSine);
        float distance_in_pixels = dist * rect_height;
        float distance_from_top_border = distance_in_pixels - half_thickness;
        float distance_from_bottom_border = distance_in_pixels + half_thickness;
        return saturate(0.5 - max(-distance_from_bottom_border, distance_from_top_border));
    }

    return 1.0;
}

// Fragment shader
fragment float4 fs_main(
    VertexOutput input [[stage_in]],
    constant Globals& globals [[buffer(1)]],
    texture2d<float> color_texture [[texture(0)]],
    texture2d<float> mask_texture [[texture(1)]],
    sampler font_sampler [[sampler(0)]]
) {
    if (input.clip_rect.z > 0.0) {
        float px = input.position.x;
        float py = input.position.y;
        if (px < input.clip_rect.x || px >= input.clip_rect.x + input.clip_rect.z ||
            py < input.clip_rect.y || py >= input.clip_rect.y + input.clip_rect.w) {
            discard_fragment();
        }
    }

    float4 out = input.f_color;

    // Handle GPU-rendered underlines
    // Underlines have: underline_style > 0, thickness in corner_radii.x
    if (input.underline_style > 0) {
        float width = input.rect_size.x;
        float rect_height = input.rect_size.y;
        float x_pos = input.f_uv.x * width;
        float y_pos = input.f_uv.y * rect_height;
        float thickness = input.corner_radii.x;

        float alpha = underline_alpha(x_pos, y_pos, rect_height, thickness, input.underline_style);
        return float4(
            prepare_output_rgb(input.f_color.rgb, globals.input_colorspace),
            input.f_color.a * alpha
        );
    }

    // Handle texture sampling for glyphs
    if (input.color_layer > 0) {
        out = color_texture.sample(font_sampler, input.f_uv, level(0.0));
    }

    if (input.mask_layer > 0) {
        float mask_alpha = mask_texture.sample(font_sampler, input.f_uv, level(0.0)).x;
        out = float4(out.xyz, input.f_color.a * mask_alpha);
    }

    // Check if we have any rounding
    bool has_corners = input.corner_radii.x != 0.0 || input.corner_radii.y != 0.0 ||
                       input.corner_radii.z != 0.0 || input.corner_radii.w != 0.0;

    // Fast path: no rounding
    if (!has_corners) {
        return float4(prepare_output_rgb(out.rgb, globals.input_colorspace), out.a);
    }

    float2 size = input.rect_size;
    float2 half_size = size / 2.0;

    // Convert UV (0-1) to local position centered at rect center
    float2 center_to_point = (input.f_uv - 0.5) * size;

    // Antialiasing threshold
    float antialias_threshold = 0.5;

    // Pick the corner radius for this quadrant
    float corner_radius = pick_corner_radius(center_to_point, input.corner_radii);

    // Vector from corner to point (mirrored to bottom-right quadrant)
    float2 corner_to_point = abs(center_to_point) - half_size;

    // Vector from corner center (for rounded corner) to point
    float2 corner_center_to_point = corner_to_point + corner_radius;

    // Outer SDF: distance to the outer edge of the quad
    float outer_sdf = quad_sdf(corner_center_to_point, corner_radius);

    // If outside the quad, discard
    if (outer_sdf >= antialias_threshold) {
        discard_fragment();
    }

    float edge = saturate(antialias_threshold - outer_sdf);
    return float4(prepare_output_rgb(out.rgb, globals.input_colorspace), out.a * edge);
}
