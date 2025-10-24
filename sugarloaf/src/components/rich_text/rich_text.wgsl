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
    @location(1) v_color: vec4<f32>,
    @location(2) v_uv: vec2<f32>,
    @location(3) layers: vec2<i32>,
    @location(4) border_radius: f32,
    @location(5) rect_size: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) f_color: vec4<f32>,
    @location(1) f_uv: vec2<f32>,
    @location(2) color_layer: i32,
    @location(3) mask_layer: i32,
    @location(4) border_radius: f32,
    @location(5) rect_size: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.f_color = input.v_color;
    out.f_uv = input.v_uv;
    out.color_layer = input.layers.x;
    out.mask_layer = input.layers.y;
    out.border_radius = input.border_radius;
    out.rect_size = input.rect_size;

    out.position = globals.transform * vec4<f32>(input.v_pos.xy, 0.0, 1.0);
    return out;
}

// Signed distance field for rounded rectangle
fn rounded_rect_sdf(corner_center_to_point: vec2<f32>, corner_radius: f32) -> f32 {
    if (corner_radius == 0.0) {
        // Fast path for unrounded corners
        return max(corner_center_to_point.x, corner_center_to_point.y);
    } else {
        // Signed distance of the point from a quad that is inset by corner_radius
        // It is negative inside this quad, and positive outside
        let signed_distance_to_inset_quad =
            // 0 inside the inset quad, and positive outside
            length(max(vec2<f32>(0.0), corner_center_to_point)) +
            // 0 outside the inset quad, and negative inside
            min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));

        return signed_distance_to_inset_quad - corner_radius;
    }
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var out: vec4<f32> = input.f_color;

    if input.color_layer > 0 {
        let tex_sample = textureSampleLevel(color_texture, font_sampler, input.f_uv, 0.0);
        out = tex_sample;
    }

    if input.mask_layer > 0 {
        let tex_alpha = textureSampleLevel(mask_texture, font_sampler, input.f_uv, 0.0).x;
        out = vec4<f32>(out.xyz, input.f_color.a * tex_alpha);
    }

    // Apply rounded rectangle mask if border_radius > 0
    var alpha_factor = 1.0;
    if (input.border_radius > 0.0) {
        let half_size = input.rect_size / 2.0;
        // Convert UV (0-1) to local position (-half_size to +half_size)
        let local_pos = (input.f_uv - 0.5) * input.rect_size;
        let corner_to_point = abs(local_pos) - half_size;
        let corner_center_to_point = corner_to_point + input.border_radius;
        let distance = rounded_rect_sdf(corner_center_to_point, input.border_radius);

        // Antialiasing: 0.5 is the threshold for pixel coverage
        alpha_factor = saturate(0.5 - distance);
    }

    return out * vec4<f32>(1.0, 1.0, 1.0, alpha_factor);
}
