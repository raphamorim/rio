struct Globals {
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var font_sampler: sampler;
@group(1) @binding(0) var color_texture: texture_2d<f32>; // RGBA texture for color glyphs
@group(1) @binding(1) var mask_texture: texture_2d<f32>;  // R8 texture for alpha masks

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) v_pos: vec3<f32>,
    @location(1) v_color: vec4<f32>,           // Background color / underline color
    @location(2) v_uv: vec2<f32>,
    @location(3) layers: vec2<i32>,
    @location(4) corner_radii: vec4<f32>,      // [top_left, top_right, bottom_right, bottom_left] / for underlines: [thickness, 0, 0, 0]
    @location(5) rect_size: vec2<f32>,         // For underlines: [width, height]
    @location(6) border_widths: vec4<f32>,     // [top, right, bottom, left]
    @location(7) border_color: vec4<f32>,      // Border color RGBA
    @location(8) border_style: i32,            // 0 = solid, 1 = dashed
    @location(9) underline_style: i32,         // 0 = none, 1 = regular, 2 = dashed, 3 = dotted, 4 = curly
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) f_color: vec4<f32>,
    @location(1) f_uv: vec2<f32>,
    @location(2) color_layer: i32,
    @location(3) mask_layer: i32,
    @location(4) corner_radii: vec4<f32>,
    @location(5) rect_size: vec2<f32>,
    @location(6) border_widths: vec4<f32>,
    @location(7) border_color: vec4<f32>,
    @location(8) @interpolate(flat) border_style: i32,
    @location(9) @interpolate(flat) underline_style: i32,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
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

    out.position = globals.transform * vec4<f32>(input.v_pos.xy, 0.0, 1.0);
    return out;
}

// Pick the corner radius based on which quadrant the point is in
fn pick_corner_radius(center_to_point: vec2<f32>, corner_radii: vec4<f32>) -> f32 {
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
fn quad_sdf(corner_center_to_point: vec2<f32>, corner_radius: f32) -> f32 {
    if (corner_radius == 0.0) {
        // Fast path for sharp corners
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        // Signed distance of the point from a quad that is inset by corner_radius.
        // It is negative inside this quad, and positive outside.
        let signed_distance_to_inset_quad =
            // 0 inside the inset quad, and positive outside.
            length(max(vec2<f32>(0.0), corner_center_to_point)) +
            // 0 outside the inset quad, and negative inside.
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));
        return signed_distance_to_inset_quad - corner_radius;
    }
}

// Approximates distance to the nearest point on a quarter ellipse.
// Sufficient for anti-aliasing when the ellipse is not very eccentric.
// The components of `point` are expected to be positive.
// Negative on the outside and positive on the inside.
fn quarter_ellipse_sdf(point: vec2<f32>, radii: vec2<f32>) -> f32 {
    // Scale the space to treat the ellipse like a unit circle.
    let circle_vec = point / radii;
    let unit_circle_sdf = length(circle_vec) - 1.0;
    // Approximate up-scaling of the length by using the average of the radii.
    return unit_circle_sdf * (radii.x + radii.y) * -0.5;
}

// Arc SDF from Inigo Quilez (https://iquilezles.org/articles/distfunctions2d/)
// p: point relative to arc center
// sc: vec2(sin(aperture), cos(aperture)) where aperture is half the arc's opening angle
// ra: arc radius
// rb: arc thickness (stroke width / 2)
fn sd_arc(p: vec2<f32>, sc: vec2<f32>, ra: f32, rb: f32) -> f32 {
    let pa = vec2<f32>(abs(p.x), p.y);
    let k = select(abs(length(pa) - ra), length(pa - sc * ra), sc.y * pa.x > sc.x * pa.y);
    return k - rb;
}

// Alpha blend: place `above` on top of `below`
fn over(below: vec4<f32>, above: vec4<f32>) -> vec4<f32> {
    let alpha = above.a + below.a * (1.0 - above.a);
    let color = (above.rgb * above.a + below.rgb * below.a * (1.0 - above.a)) / alpha;
    return vec4<f32>(color, alpha);
}

const M_PI_F: f32 = 3.1415926;

// Modulus that has the same sign as `a`.
fn fmod(a: f32, b: f32) -> f32 {
    return a - b * trunc(a / b);
}

// Returns the dash velocity of a corner given the dash velocity of the two
// sides, by returning the slower velocity (larger dashes).
fn corner_dash_velocity(dv1: f32, dv2: f32) -> f32 {
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
fn dash_alpha(t: f32, period: f32, dash_length: f32, dash_velocity: f32, antialias_threshold: f32) -> f32 {
    let half_period = period / 2.0;
    let half_length = dash_length / 2.0;
    // Value in [-half_period, half_period].
    // The dash is in [-half_length, half_length].
    let centered = fmod(t + half_period - half_length, period) - half_period;
    // Signed distance for the dash, negative values are inside the dash.
    let signed_distance = abs(centered) - half_length;
    // Antialiased alpha based on the signed distance.
    return saturate(antialias_threshold - signed_distance / dash_velocity);
}

// Calculate underline alpha for pattern rendering
fn underline_alpha(x_pos: f32, y_pos: f32, rect_height: f32, thickness: f32, style: i32) -> f32 {
    // style 1: regular solid line
    if (style == 1) {
        return 1.0;
    }

    // style 2: dashed (6px dash, 2px gap)
    if (style == 2) {
        let antialias = 0.5;
        let dash_width = 6.0;
        let gap_width = 2.0;
        let period = dash_width + gap_width;
        let pos_in_period = fmod(x_pos, period);
        let start_aa = saturate(pos_in_period / antialias);
        let end_aa = saturate((dash_width - pos_in_period) / antialias);
        return min(start_aa, end_aa);
    }

    // style 3: dotted (2px dot, 2px gap)
    if (style == 3) {
        let antialias = 0.5;
        let dot_width = 2.0;
        let gap_width = 2.0;
        let period = dot_width + gap_width;
        let pos_in_period = fmod(x_pos, period);
        let start_aa = saturate(pos_in_period / antialias);
        let end_aa = saturate((dot_width - pos_in_period) / antialias);
        return min(start_aa, end_aa);
    }

    // style 4: curly (sine wave) using SDF
    if (style == 4) {
        let WAVE_FREQUENCY: f32 = 2.0;
        let WAVE_HEIGHT_RATIO: f32 = 0.8;

        let half_thickness = thickness * 0.5;
        let st = vec2<f32>(x_pos / rect_height, y_pos / rect_height - 0.5);
        let frequency = M_PI_F * WAVE_FREQUENCY * thickness / rect_height;
        let amplitude = (thickness * WAVE_HEIGHT_RATIO) / rect_height;

        let sine = sin(st.x * frequency) * amplitude;
        let dSine = cos(st.x * frequency) * amplitude * frequency;
        let distance = (st.y - sine) / sqrt(1.0 + dSine * dSine);
        let distance_in_pixels = distance * rect_height;
        let distance_from_top_border = distance_in_pixels - half_thickness;
        let distance_from_bottom_border = distance_in_pixels + half_thickness;
        return saturate(0.5 - max(-distance_from_bottom_border, distance_from_top_border));
    }

    return 1.0;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var out: vec4<f32> = input.f_color;

    // Handle GPU-rendered underlines
    // Underlines have: underline_style > 0, thickness in corner_radii.x
    if (input.underline_style > 0) {
        let width = input.rect_size.x;
        let rect_height = input.rect_size.y;
        let x_pos = input.f_uv.x * width;
        let y_pos = input.f_uv.y * rect_height;
        let thickness = input.corner_radii.x;

        let alpha = underline_alpha(x_pos, y_pos, rect_height, thickness, input.underline_style);
        return vec4<f32>(input.f_color.rgb, input.f_color.a * alpha);
    }

    // Handle GPU-rendered arcs (border_style == -1)
    // Arc params: corner_radii.x = radius, corner_radii.y = stroke_width
    //             corner_radii.z = sin(aperture), corner_radii.w = cos(aperture)
    //             border_widths.x = rotation angle (radians)
    //             border_widths.yz = arc center offset from box center
    if (input.border_style == -1) {
        let size = input.rect_size;
        // Convert UV (0-1) to local position centered at arc center
        let arc_offset = vec2<f32>(input.border_widths.y, input.border_widths.z);
        let local_pos = (input.f_uv - 0.5) * size - arc_offset;

        // Apply rotation for arc orientation
        let rot_angle = input.border_widths.x;
        let cos_r = cos(rot_angle);
        let sin_r = sin(rot_angle);
        let rotated_pos = vec2<f32>(
            local_pos.x * cos_r - local_pos.y * sin_r,
            local_pos.x * sin_r + local_pos.y * cos_r
        );

        let radius = input.corner_radii.x;
        let stroke_width = input.corner_radii.y;
        let sc = vec2<f32>(input.corner_radii.z, input.corner_radii.w);

        let dist = sd_arc(rotated_pos, sc, radius, stroke_width / 2.0);

        // Antialiasing
        let alpha = 1.0 - smoothstep(-0.5, 0.5, dist);

        if (alpha < 0.01) {
            discard;
        }

        return vec4<f32>(input.f_color.rgb, input.f_color.a * alpha);
    }

    // Handle texture sampling for glyphs
    if input.color_layer > 0 {
        let tex_sample = textureSampleLevel(color_texture, font_sampler, input.f_uv, 0.0);
        out = tex_sample;
    }

    if input.mask_layer > 0 {
        let tex_alpha = textureSampleLevel(mask_texture, font_sampler, input.f_uv, 0.0).x;
        out = vec4<f32>(out.xyz, input.f_color.a * tex_alpha);
    }

    // Check if we have any rounding or borders
    let has_corners = input.corner_radii.x != 0.0 || input.corner_radii.y != 0.0 ||
                      input.corner_radii.z != 0.0 || input.corner_radii.w != 0.0;
    let has_borders = input.border_widths.x != 0.0 || input.border_widths.y != 0.0 ||
                      input.border_widths.z != 0.0 || input.border_widths.w != 0.0;

    // Fast path: no rounding and no borders
    if (!has_corners && !has_borders) {
        return out;
    }

    let size = input.rect_size;
    let half_size = size / 2.0;

    // Convert UV (0-1) to local position centered at rect center
    let local_pos = (input.f_uv - 0.5) * size;
    let center_to_point = local_pos;

    // Antialiasing threshold
    let antialias_threshold = 0.5;

    // Pick the corner radius for this quadrant
    let corner_radius = pick_corner_radius(center_to_point, input.corner_radii);

    // Pick the border widths for this quadrant
    let border = vec2<f32>(
        select(input.border_widths.y, input.border_widths.w, center_to_point.x < 0.0), // right or left
        select(input.border_widths.z, input.border_widths.x, center_to_point.y < 0.0)  // bottom or top
    );

    // Vector from corner to point (mirrored to bottom-right quadrant)
    let corner_to_point = abs(center_to_point) - half_size;

    // Vector from corner center (for rounded corner) to point
    let corner_center_to_point = corner_to_point + corner_radius;

    // Check if near rounded corner
    let is_near_rounded_corner = corner_center_to_point.x >= 0.0 && corner_center_to_point.y >= 0.0;

    // Outer SDF: distance to the outer edge of the quad
    let outer_sdf = quad_sdf(corner_center_to_point, corner_radius);

    // If outside the quad, discard
    if (outer_sdf >= antialias_threshold) {
        discard;
    }

    // 0-width borders are reduced so that `inner_sdf >= antialias_threshold`.
    // The purpose of this is to not draw antialiasing pixels in this case.
    let reduced_border = vec2<f32>(
        select(border.x, -antialias_threshold, border.x == 0.0),
        select(border.y, -antialias_threshold, border.y == 0.0)
    );

    // Vector from straight border inner corner to point.
    let straight_border_inner_corner_to_point = corner_to_point + reduced_border;

    // Whether the point is beyond the inner edge of the straight border.
    let is_beyond_inner_straight_border =
        straight_border_inner_corner_to_point.x > 0.0 ||
        straight_border_inner_corner_to_point.y > 0.0;

    // Whether the point is far enough inside the quad, such that the pixels are
    // not affected by the straight border.
    let is_within_inner_straight_border =
        straight_border_inner_corner_to_point.x < -antialias_threshold &&
        straight_border_inner_corner_to_point.y < -antialias_threshold;

    // Fast path for points that must be part of the background.
    if (is_within_inner_straight_border && !is_near_rounded_corner) {
        return input.f_color;
    }

    // Approximate signed distance of the point to the inside edge of the quad's
    // border. It is negative outside this edge (within the border), and
    // positive inside.
    var inner_sdf = 0.0;
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
        let ellipse_radii = max(vec2<f32>(0.0), vec2<f32>(corner_radius) - reduced_border);
        inner_sdf = quarter_ellipse_sdf(corner_center_to_point, ellipse_radii);
    }

    // Negative when inside the border
    let border_sdf = max(inner_sdf, outer_sdf);

    // Check if we have corners
    let unrounded = input.corner_radii.x == 0.0 &&
        input.corner_radii.y == 0.0 &&
        input.corner_radii.z == 0.0 &&
        input.corner_radii.w == 0.0;

    var color = input.f_color;
    if (border_sdf < antialias_threshold) {
        var border_color = input.border_color;

        // Dashed border logic when border_style == 1
        if (input.border_style == 1) {
            // Position along the perimeter in "dash space"
            var t = 0.0;
            var max_t = 0.0;

            // Border width is proportional to dash size
            // Dash pattern: (2 * border width) dash, (1 * border width) gap
            let dash_length_per_width = 2.0;
            let dash_gap_per_width = 1.0;
            let dash_period_per_width = dash_length_per_width + dash_gap_per_width;

            // Dash velocity = dash periods per pixel
            var dash_velocity = 0.0;
            let dv_numerator = 1.0 / dash_period_per_width;

            // Convert UV to point position relative to bounds origin
            let point = input.f_uv * size;

            if (unrounded) {
                // For unrounded corners, dashes are laid out separately on each side
                let is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
                let border_width = select(border.y, border.x, is_horizontal);
                dash_velocity = dv_numerator / border_width;
                t = select(point.y, point.x, is_horizontal) * dash_velocity;
                max_t = select(size.y, size.x, is_horizontal) * dash_velocity;
            } else {
                // For rounded corners, dashes flow around the entire perimeter
                let r_tr = input.corner_radii.y;
                let r_br = input.corner_radii.z;
                let r_bl = input.corner_radii.w;
                let r_tl = input.corner_radii.x;

                let w_t = input.border_widths.x;
                let w_r = input.border_widths.y;
                let w_b = input.border_widths.z;
                let w_l = input.border_widths.w;

                // Straight side dash velocities
                let dv_t = select(dv_numerator / w_t, 0.0, w_t <= 0.0);
                let dv_r = select(dv_numerator / w_r, 0.0, w_r <= 0.0);
                let dv_b = select(dv_numerator / w_b, 0.0, w_b <= 0.0);
                let dv_l = select(dv_numerator / w_l, 0.0, w_l <= 0.0);

                // Straight side lengths in dash space
                let s_t = (size.x - r_tl - r_tr) * dv_t;
                let s_r = (size.y - r_tr - r_br) * dv_r;
                let s_b = (size.x - r_br - r_bl) * dv_b;
                let s_l = (size.y - r_bl - r_tl) * dv_l;

                let corner_dv_tr = corner_dash_velocity(dv_t, dv_r);
                let corner_dv_br = corner_dash_velocity(dv_b, dv_r);
                let corner_dv_bl = corner_dash_velocity(dv_b, dv_l);
                let corner_dv_tl = corner_dash_velocity(dv_t, dv_l);

                // Corner lengths in dash space
                let c_tr = r_tr * (M_PI_F / 2.0) * corner_dv_tr;
                let c_br = r_br * (M_PI_F / 2.0) * corner_dv_br;
                let c_bl = r_bl * (M_PI_F / 2.0) * corner_dv_bl;
                let c_tl = r_tl * (M_PI_F / 2.0) * corner_dv_tl;

                // Cumulative dash space up to each segment
                let upto_tr = s_t;
                let upto_r = upto_tr + c_tr;
                let upto_br = upto_r + s_r;
                let upto_b = upto_br + c_br;
                let upto_bl = upto_b + s_b;
                let upto_l = upto_bl + c_bl;
                let upto_tl = upto_l + s_l;
                max_t = upto_tl + c_tl;

                if (is_near_rounded_corner) {
                    let radians = atan2(corner_center_to_point.y, corner_center_to_point.x);
                    let corner_t = radians * corner_radius;

                    if (center_to_point.x >= 0.0) {
                        if (center_to_point.y < 0.0) {
                            dash_velocity = corner_dv_tr;
                            t = upto_r - corner_t * dash_velocity;
                        } else {
                            dash_velocity = corner_dv_br;
                            t = upto_br + corner_t * dash_velocity;
                        }
                    } else {
                        if (center_to_point.y >= 0.0) {
                            dash_velocity = corner_dv_bl;
                            t = upto_l - corner_t * dash_velocity;
                        } else {
                            dash_velocity = corner_dv_tl;
                            t = upto_tl + corner_t * dash_velocity;
                        }
                    }
                } else {
                    // Straight borders
                    let is_horizontal = corner_center_to_point.x < corner_center_to_point.y;
                    if (is_horizontal) {
                        if (center_to_point.y < 0.0) {
                            dash_velocity = dv_t;
                            t = (point.x - r_tl) * dash_velocity;
                        } else {
                            dash_velocity = dv_b;
                            t = upto_bl - (point.x - r_bl) * dash_velocity;
                        }
                    } else {
                        if (center_to_point.x < 0.0) {
                            dash_velocity = dv_l;
                            t = upto_tl - (point.y - r_tl) * dash_velocity;
                        } else {
                            dash_velocity = dv_r;
                            t = upto_r + (point.y - r_tr) * dash_velocity;
                        }
                    }
                }
            }

            let dash_len = dash_length_per_width / dash_period_per_width;

            // Straight borders should start and end with a dash
            max_t -= select(0.0, dash_len, unrounded);
            if (max_t >= 1.0) {
                let dash_count = floor(max_t);
                let dash_period = max_t / dash_count;
                border_color.a *= dash_alpha(t, dash_period, dash_len, dash_velocity, antialias_threshold);
            } else if (unrounded) {
                let dash_gap = max_t - dash_len;
                if (dash_gap > 0.0) {
                    let dash_period = dash_len + dash_gap;
                    border_color.a *= dash_alpha(t, dash_period, dash_len, dash_velocity, antialias_threshold);
                }
            }
        }

        // Blend the border on top of the background and then linearly interpolate
        // between the two as we slide inside the background.
        let blended_border = over(input.f_color, border_color);
        color = mix(input.f_color, blended_border,
                    saturate(antialias_threshold - inner_sdf));
    }

    return color * vec4<f32>(1.0, 1.0, 1.0, saturate(antialias_threshold - outer_sdf));
}
