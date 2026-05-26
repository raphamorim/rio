struct Globals {
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var font_sampler: sampler;
@group(1) @binding(0) var color_texture: texture_2d<f32>; // RGBA texture for color glyphs
@group(1) @binding(1) var mask_texture: texture_2d<f32>;  // R8 texture for alpha masks

// Per-instance data for instanced quad rendering (matches Rust QuadInstance)
struct QuadInstanceInput {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv_rect: vec4<f32>,
    @location(3) layers: vec2<i32>,
    @location(4) size: vec2<f32>,
    @location(5) corner_radii: vec4<f32>,
    @location(6) underline_style: i32,
    @location(7) clip_rect: vec4<f32>,
}

// Per-vertex data for non-quad geometry (lines, triangles, arcs)
struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) v_pos: vec3<f32>,
    @location(1) v_color: vec4<f32>,
    @location(2) v_uv: vec2<f32>,
    @location(3) layers: vec2<i32>,
    @location(4) corner_radii: vec4<f32>,
    @location(5) rect_size: vec2<f32>,
    @location(6) underline_style: i32,
    @location(7) clip_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) f_color: vec4<f32>,
    @location(1) f_uv: vec2<f32>,
    @location(2) color_layer: i32,
    @location(3) mask_layer: i32,
    @location(4) corner_radii: vec4<f32>,
    @location(5) rect_size: vec2<f32>,
    @location(6) @interpolate(flat) underline_style: i32,
    @location(7) @interpolate(flat) clip_rect: vec4<f32>,
}

// Unit quad corners for triangle strip: TL, BL, TR, BR
const UNIT_QUAD = array<vec2<f32>, 4>(
    vec2<f32>(0.0, 0.0),
    vec2<f32>(0.0, 1.0),
    vec2<f32>(1.0, 0.0),
    vec2<f32>(1.0, 1.0),
);

@vertex
fn vs_instanced(
    @builtin(vertex_index) vertex_id: u32,
    input: QuadInstanceInput,
) -> VertexOutput {
    let unit = UNIT_QUAD[vertex_id];
    let pos = input.pos.xy + unit * input.size;
    let uv = mix(input.uv_rect.xy, input.uv_rect.zw, unit);

    var out: VertexOutput;
    out.position = globals.transform * vec4<f32>(pos, input.pos.z, 1.0);
    out.f_color = input.color;
    out.f_uv = uv;
    out.color_layer = input.layers.x;
    out.mask_layer = input.layers.y;
    out.corner_radii = input.corner_radii;
    out.rect_size = input.size;
    out.underline_style = input.underline_style;
    out.clip_rect = input.clip_rect;
    return out;
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
    out.underline_style = input.underline_style;
    out.clip_rect = input.clip_rect;

    out.position = globals.transform * vec4<f32>(input.v_pos, 1.0);
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

const M_PI_F: f32 = 3.1415926;

// Modulus that has the same sign as `a`.
fn fmod(a: f32, b: f32) -> f32 {
    return a - b * trunc(a / b);
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
    if (input.clip_rect.z > 0.0) {
        let px = input.position.x;
        let py = input.position.y;
        if (px < input.clip_rect.x || px >= input.clip_rect.x + input.clip_rect.z ||
            py < input.clip_rect.y || py >= input.clip_rect.y + input.clip_rect.w) {
            discard;
        }
    }

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

    // Handle texture sampling for glyphs
    if input.color_layer > 0 {
        let tex_sample = textureSampleLevel(color_texture, font_sampler, input.f_uv, 0.0);
        out = tex_sample;
    }

    if input.mask_layer > 0 {
        let tex_alpha = textureSampleLevel(mask_texture, font_sampler, input.f_uv, 0.0).x;
        out = vec4<f32>(out.xyz, input.f_color.a * tex_alpha);
    }

    // Check if we have any rounding
    let has_corners = input.corner_radii.x != 0.0 || input.corner_radii.y != 0.0 ||
                      input.corner_radii.z != 0.0 || input.corner_radii.w != 0.0;

    // Fast path: no rounding
    if (!has_corners) {
        return out;
    }

    let size = input.rect_size;
    let half_size = size / 2.0;

    // Convert UV (0-1) to local position centered at rect center
    let center_to_point = (input.f_uv - 0.5) * size;

    // Antialiasing threshold
    let antialias_threshold = 0.5;

    // Pick the corner radius for this quadrant
    let corner_radius = pick_corner_radius(center_to_point, input.corner_radii);

    // Vector from corner to point (mirrored to bottom-right quadrant)
    let corner_to_point = abs(center_to_point) - half_size;

    // Vector from corner center (for rounded corner) to point
    let corner_center_to_point = corner_to_point + corner_radius;

    // Outer SDF: distance to the outer edge of the quad
    let outer_sdf = quad_sdf(corner_center_to_point, corner_radius);

    // If outside the quad, discard
    if (outer_sdf >= antialias_threshold) {
        discard;
    }

    return out * vec4<f32>(1.0, 1.0, 1.0, saturate(antialias_threshold - outer_sdf));
}
