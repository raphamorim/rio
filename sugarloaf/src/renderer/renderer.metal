#include <metal_stdlib>
using namespace metal;

// Uniform buffer structure (equivalent to @group(0) @binding(0))
struct Globals {
    float4x4 transform;
};

// Vertex input structure - matches Vertex struct exactly
struct VertexInput {
    float3 v_pos [[attribute(0)]];          // Position (12 bytes)
    float4 v_color [[attribute(1)]];        // Background color / underline color (16 bytes)
    float2 v_uv [[attribute(2)]];           // UV coords (8 bytes)
    int2 layers [[attribute(3)]];           // Layers (8 bytes)
    float4 corner_radii [[attribute(4)]];   // Corner radii / for underlines: [thickness, 0, 0, 0] (16 bytes)
    float2 rect_size [[attribute(5)]];      // Rect size / underline [width, height] (8 bytes)
    float4 border_widths [[attribute(6)]];  // Border widths [top, right, bottom, left] (16 bytes)
    float4 border_color [[attribute(7)]];   // Border color RGBA (16 bytes)
    int border_style [[attribute(8)]];      // 0 = solid, 1 = dashed (4 bytes)
    int underline_style [[attribute(9)]];   // 0 = none, 1 = regular, 2 = dashed, 3 = dotted, 4 = curly (4 bytes)
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
    float4 border_widths;
    float4 border_color;
    int border_style [[flat]];
    int underline_style [[flat]];
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
    out.border_widths = input.border_widths;
    out.border_color = input.border_color;
    out.border_style = input.border_style;
    out.underline_style = input.underline_style;

    // Transform position - use float4 constructor with z=0.0, w=1.0
    out.position = globals.transform * float4(input.v_pos.xy, 0.0, 1.0);

    return out;
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

// Approximates distance to the nearest point on a quarter ellipse.
// Sufficient for anti-aliasing when the ellipse is not very eccentric.
// The components of `point` are expected to be positive.
// Negative on the outside and positive on the inside.
float quarter_ellipse_sdf(float2 point, float2 radii) {
    // Scale the space to treat the ellipse like a unit circle.
    float2 circle_vec = point / radii;
    float unit_circle_sdf = length(circle_vec) - 1.0;
    // Approximate up-scaling of the length by using the average of the radii.
    return unit_circle_sdf * (radii.x + radii.y) * -0.5;
}

// Alpha blend: place `above` on top of `below`
float4 over(float4 below, float4 above) {
    float alpha = above.a + below.a * (1.0 - above.a);
    float3 color = (above.rgb * above.a + below.rgb * below.a * (1.0 - above.a)) / alpha;
    return float4(color, alpha);
}

constant float PI_F = 3.1415926;

// Modulus that has the same sign as `a`.
float fmod_pos(float a, float b) {
    return a - b * trunc(a / b);
}

// Returns the dash velocity of a corner given the dash velocity of the two
// sides, by returning the slower velocity (larger dashes).
float corner_dash_velocity(float dv1, float dv2) {
    if (dv1 == 0.0) {
        return dv2;
    } else if (dv2 == 0.0) {
        return dv1;
    } else {
        return min(dv1, dv2);
    }
}

// Returns alpha used to render antialiased dashes.
// `t` is within the dash when `fmod(t, period) < length`.
float dash_alpha(float t, float period, float dash_length, float dash_velocity, float antialias_threshold) {
    float half_period = period / 2.0;
    float half_length = dash_length / 2.0;
    // Value in [-half_period, half_period].
    // The dash is in [-half_length, half_length].
    float centered = fmod_pos(t + half_period - half_length, period) - half_period;
    // Signed distance for the dash, negative values are inside the dash.
    float signed_distance = abs(centered) - half_length;
    // Antialiased alpha based on the signed distance.
    return saturate(antialias_threshold - signed_distance / dash_velocity);
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
    texture2d<float> color_texture [[texture(0)]],
    texture2d<float> mask_texture [[texture(1)]],
    sampler font_sampler [[sampler(0)]]
) {
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
        return float4(input.f_color.rgb, input.f_color.a * alpha);
    }

    // Handle texture sampling for glyphs
    if (input.color_layer > 0) {
        out = color_texture.sample(font_sampler, input.f_uv, level(0.0));
    }

    if (input.mask_layer > 0) {
        float mask_alpha = mask_texture.sample(font_sampler, input.f_uv, level(0.0)).x;
        out = float4(out.xyz, input.f_color.a * mask_alpha);
    }

    // Check if we have any rounding or borders
    bool has_corners = input.corner_radii.x != 0.0 || input.corner_radii.y != 0.0 ||
                       input.corner_radii.z != 0.0 || input.corner_radii.w != 0.0;
    bool has_borders = input.border_widths.x != 0.0 || input.border_widths.y != 0.0 ||
                       input.border_widths.z != 0.0 || input.border_widths.w != 0.0;

    // Fast path: no rounding and no borders
    if (!has_corners && !has_borders) {
        return out;
    }

    float2 size = input.rect_size;
    float2 half_size = size / 2.0;

    // Convert UV (0-1) to local position centered at rect center
    float2 local_pos = (input.f_uv - 0.5) * size;
    float2 center_to_point = local_pos;

    // Antialiasing threshold
    float antialias_threshold = 0.5;

    // Pick the corner radius for this quadrant
    float corner_radius = pick_corner_radius(center_to_point, input.corner_radii);

    // Pick the border widths for this quadrant
    float2 border = float2(
        center_to_point.x < 0.0 ? input.border_widths.w : input.border_widths.y, // left or right
        center_to_point.y < 0.0 ? input.border_widths.x : input.border_widths.z  // top or bottom
    );

    // Vector from corner to point (mirrored to bottom-right quadrant)
    float2 corner_to_point = abs(center_to_point) - half_size;

    // Vector from corner center (for rounded corner) to point
    float2 corner_center_to_point = corner_to_point + corner_radius;

    // Check if near rounded corner
    bool is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;

    // Outer SDF: distance to the outer edge of the quad
    float outer_sdf = quad_sdf(corner_center_to_point, corner_radius);

    // If outside the quad, discard
    if (outer_sdf >= antialias_threshold) {
        discard_fragment();
    }

    // 0-width borders are reduced so that `inner_sdf >= antialias_threshold`.
    // The purpose of this is to not draw antialiasing pixels in this case.
    float2 reduced_border = float2(
        border.x == 0.0 ? -antialias_threshold : border.x,
        border.y == 0.0 ? -antialias_threshold : border.y
    );

    // Vector from straight border inner corner to point.
    float2 straight_border_inner_corner_to_point = corner_to_point + reduced_border;

    // Whether the point is beyond the inner edge of the straight border.
    bool is_beyond_inner_straight_border =
        straight_border_inner_corner_to_point.x > 0.0 ||
        straight_border_inner_corner_to_point.y > 0.0;

    // Whether the point is far enough inside the quad, such that the pixels are
    // not affected by the straight border.
    bool is_within_inner_straight_border =
        straight_border_inner_corner_to_point.x < -antialias_threshold &&
        straight_border_inner_corner_to_point.y < -antialias_threshold;

    // Fast path for points that must be part of the background.
    if (is_within_inner_straight_border && !is_near_rounded_corner) {
        return input.f_color;
    }

    // Approximate signed distance of the point to the inside edge of the quad's
    // border. It is negative outside this edge (within the border), and
    // positive inside.
    float inner_sdf = 0.0;
    if (corner_center_to_point.x <= 0.0 || corner_center_to_point.y <= 0.0) {
        // Fast path for straight borders.
        inner_sdf = -max(straight_border_inner_corner_to_point.x,
                         straight_border_inner_corner_to_point.y);
    } else if (is_beyond_inner_straight_border) {
        // Fast path for points that must be outside the inner edge.
        inner_sdf = -1.0;
    } else if (reduced_border.x == reduced_border.y) {
        // Fast path for circular inner edge.
        inner_sdf = -(outer_sdf + reduced_border.x);
    } else {
        // Elliptical inner edge - use quarter_ellipse_sdf for accuracy.
        float2 ellipse_radii = max(float2(0.0), float2(corner_radius) - reduced_border);
        inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
    }

    // Negative when inside the border
    float border_sdf = max(inner_sdf, outer_sdf);

    // Check if we have corners
    bool unrounded = input.corner_radii.x == 0.0 &&
        input.corner_radii.y == 0.0 &&
        input.corner_radii.z == 0.0 &&
        input.corner_radii.w == 0.0;

    float4 color = input.f_color;
    if (border_sdf < antialias_threshold) {
        float4 border_color = input.border_color;

        // Dashed border logic when border_style == 1
        if (input.border_style == 1) {
            // Position along the perimeter in "dash space"
            float t = 0.0;
            float max_t = 0.0;

            // Border width is proportional to dash size
            // Dash pattern: (2 * border width) dash, (1 * border width) gap
            float dash_length_per_width = 2.0;
            float dash_gap_per_width = 1.0;
            float dash_period_per_width = dash_length_per_width + dash_gap_per_width;

            // Dash velocity = dash periods per pixel
            float dash_velocity_val = 0.0;
            float dv_numerator = 1.0 / dash_period_per_width;

            // Convert UV to point position relative to bounds origin
            float2 point = input.f_uv * size;

            if (unrounded) {
                // For unrounded corners, dashes are laid out separately on each side
                bool is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
                float border_width = is_horizontal ? border.x : border.y;
                dash_velocity_val = dv_numerator / border_width;
                t = (is_horizontal ? point.x : point.y) * dash_velocity_val;
                max_t = (is_horizontal ? size.x : size.y) * dash_velocity_val;
            } else {
                // For rounded corners, dashes flow around the entire perimeter
                float r_tr = input.corner_radii.y;
                float r_br = input.corner_radii.z;
                float r_bl = input.corner_radii.w;
                float r_tl = input.corner_radii.x;

                float w_t = input.border_widths.x;
                float w_r = input.border_widths.y;
                float w_b = input.border_widths.z;
                float w_l = input.border_widths.w;

                // Straight side dash velocities
                float dv_t = w_t <= 0.0 ? 0.0 : dv_numerator / w_t;
                float dv_r = w_r <= 0.0 ? 0.0 : dv_numerator / w_r;
                float dv_b = w_b <= 0.0 ? 0.0 : dv_numerator / w_b;
                float dv_l = w_l <= 0.0 ? 0.0 : dv_numerator / w_l;

                // Straight side lengths in dash space
                float s_t = (size.x - r_tl - r_tr) * dv_t;
                float s_r = (size.y - r_tr - r_br) * dv_r;
                float s_b = (size.x - r_br - r_bl) * dv_b;
                float s_l = (size.y - r_bl - r_tl) * dv_l;

                float corner_dv_tr = corner_dash_velocity(dv_t, dv_r);
                float corner_dv_br = corner_dash_velocity(dv_b, dv_r);
                float corner_dv_bl = corner_dash_velocity(dv_b, dv_l);
                float corner_dv_tl = corner_dash_velocity(dv_t, dv_l);

                // Corner lengths in dash space
                float c_tr = r_tr * (PI_F / 2.0) * corner_dv_tr;
                float c_br = r_br * (PI_F / 2.0) * corner_dv_br;
                float c_bl = r_bl * (PI_F / 2.0) * corner_dv_bl;
                float c_tl = r_tl * (PI_F / 2.0) * corner_dv_tl;

                // Cumulative dash space up to each segment
                float upto_tr = s_t;
                float upto_r = upto_tr + c_tr;
                float upto_br = upto_r + s_r;
                float upto_b = upto_br + c_br;
                float upto_bl = upto_b + s_b;
                float upto_l = upto_bl + c_bl;
                float upto_tl = upto_l + s_l;
                max_t = upto_tl + c_tl;

                if (is_near_rounded_corner) {
                    float radians = atan2(corner_center_to_point.y, corner_center_to_point.x);
                    float corner_t = radians * corner_radius;

                    if (center_to_point.x >= 0.0) {
                        if (center_to_point.y < 0.0) {
                            dash_velocity_val = corner_dv_tr;
                            t = upto_r - corner_t * dash_velocity_val;
                        } else {
                            dash_velocity_val = corner_dv_br;
                            t = upto_br + corner_t * dash_velocity_val;
                        }
                    } else {
                        if (center_to_point.y >= 0.0) {
                            dash_velocity_val = corner_dv_bl;
                            t = upto_l - corner_t * dash_velocity_val;
                        } else {
                            dash_velocity_val = corner_dv_tl;
                            t = upto_tl + corner_t * dash_velocity_val;
                        }
                    }
                } else {
                    // Straight borders
                    bool is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
                    if (is_horizontal) {
                        if (center_to_point.y < 0.0) {
                            dash_velocity_val = dv_t;
                            t = (point.x - r_tl) * dash_velocity_val;
                        } else {
                            dash_velocity_val = dv_b;
                            t = upto_bl - (point.x - r_bl) * dash_velocity_val;
                        }
                    } else {
                        if (center_to_point.x < 0.0) {
                            dash_velocity_val = dv_l;
                            t = upto_tl - (point.y - r_tl) * dash_velocity_val;
                        } else {
                            dash_velocity_val = dv_r;
                            t = upto_r + (point.y - r_tr) * dash_velocity_val;
                        }
                    }
                }
            }

            float dash_len = dash_length_per_width / dash_period_per_width;

            // Straight borders should start and end with a dash
            max_t -= unrounded ? dash_len : 0.0;
            if (max_t >= 1.0) {
                float dash_count = floor(max_t);
                float dash_period = max_t / dash_count;
                border_color.a *= dash_alpha(t, dash_period, dash_len, dash_velocity_val, antialias_threshold);
            } else if (unrounded) {
                float dash_gap = max_t - dash_len;
                if (dash_gap > 0.0) {
                    float dash_period = dash_len + dash_gap;
                    border_color.a *= dash_alpha(t, dash_period, dash_len, dash_velocity_val, antialias_threshold);
                }
            }
        }

        // Blend the border on top of the background and then linearly interpolate
        // between the two as we slide inside the background.
        float4 blended_border = over(input.f_color, border_color);
        color = mix(input.f_color, blended_border,
                    saturate(antialias_threshold - inner_sdf));
    }

    return color * float4(1.0, 1.0, 1.0, saturate(antialias_threshold - outer_sdf));
}
