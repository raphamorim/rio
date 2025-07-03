// Simplified backdrop blur effect for Sugarloaf
// Creates a frosted glass appearance without requiring background texture capture

struct Globals {
    transform: mat4x4<f32>,
    scale: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;

fn distance_alg(
    frag_coord: vec2<f32>,
    position: vec2<f32>,
    size: vec2<f32>,
    radius: f32
) -> f32 {
    var inner_half_size: vec2<f32> = (size - vec2<f32>(radius, radius) * 2.0) / 2.0;
    var top_left: vec2<f32> = position + vec2<f32>(radius, radius);
    return rounded_box_sdf(frag_coord - top_left - inner_half_size, inner_half_size, 0.0);
}

// Given a vector from a point to the center of a rounded rectangle of the given `size` and
// border `radius`, determines the point's distance from the nearest edge of the rounded rectangle
fn rounded_box_sdf(to_center: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    return length(max(abs(to_center) - size + vec2<f32>(radius, radius), vec2<f32>(0.0, 0.0))) - radius;
}

// Based on the fragment position and the center of the quad, select one of the 4 radii.
// Order matches CSS border radius attribute:
// radii.x = top-left, radii.y = top-right, radii.z = bottom-right, radii.w = bottom-left
fn select_border_radius(radii: vec4<f32>, position: vec2<f32>, center: vec2<f32>) -> f32 {
    var rx = radii.x;
    var ry = radii.y;
    rx = select(radii.x, radii.y, position.x > center.x);
    ry = select(radii.w, radii.z, position.x > center.x);
    rx = select(rx, ry, position.y > center.y);
    return rx;
}

// Compute the normalized quad coordinates based on the vertex index.
fn vertex_position(vertex_index: u32) -> vec2<f32> {
    // #: 0 1 2 3 4 5
    // x: 1 1 0 0 0 1
    // y: 1 0 0 0 1 1
    return vec2<f32>((vec2(1u, 2u) + vertex_index) % vec2(6u) < vec2(3u));
}

// Generate pseudo-random noise for frosted glass effect
fn noise(uv: vec2<f32>) -> f32 {
    return fract(sin(dot(uv, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

// Create a frosted glass texture effect
fn frosted_glass_effect(uv: vec2<f32>, blur_strength: f32) -> vec3<f32> {
    // Create multiple layers of noise at different scales
    let noise1 = noise(uv * 10.0) * 0.5;
    let noise2 = noise(uv * 25.0) * 0.3;
    let noise3 = noise(uv * 50.0) * 0.2;
    
    let combined_noise = (noise1 + noise2 + noise3) * blur_strength * 0.1;
    
    // Create a subtle gradient effect
    let gradient = length(uv - 0.5) * 0.1;
    
    // Combine for frosted glass appearance
    let base_brightness = 0.95 + combined_noise - gradient;
    return vec3<f32>(base_brightness, base_brightness, base_brightness);
}

struct BlurVertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) color: vec4<f32>,
    @location(1) pos: vec2<f32>,
    @location(2) scale: vec2<f32>,
    @location(3) border_color: vec4<f32>,
    @location(4) border_radius: vec4<f32>,
    @location(5) border_width: f32,
    @location(6) blur_radius: f32,
}

struct BlurVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) border_color: vec4<f32>,
    @location(2) pos: vec2<f32>,
    @location(3) scale: vec2<f32>,
    @location(4) border_radius: vec4<f32>,
    @location(5) border_width: f32,
    @location(6) blur_radius: f32,
    @location(7) uv: vec2<f32>,
}

@vertex
fn frosted_glass_vs_main(input: BlurVertexInput) -> BlurVertexOutput {
    var out: BlurVertexOutput;

    var pos: vec2<f32> = input.pos * globals.scale;
    var scale: vec2<f32> = input.scale * globals.scale;
    var snap: vec2<f32> = vec2<f32>(0.0, 0.0);

    if input.scale.x == 1.0 {
        snap.x = round(pos.x) - pos.x;
    }

    if input.scale.y == 1.0 {
        snap.y = round(pos.y) - pos.y;
    }

    var min_border_radius = min(input.scale.x, input.scale.y) * 0.5;
    var border_radius: vec4<f32> = vec4<f32>(
        min(input.border_radius.x, min_border_radius),
        min(input.border_radius.y, min_border_radius),
        min(input.border_radius.z, min_border_radius),
        min(input.border_radius.w, min_border_radius)
    );

    var transform: mat4x4<f32> = mat4x4<f32>(
        vec4<f32>(scale.x + 1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, scale.y + 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(pos - vec2<f32>(0.5, 0.5) + snap, 0.0, 1.0)
    );

    let vertex_pos = vertex_position(input.vertex_index);
    out.position = globals.transform * transform * vec4<f32>(vertex_pos, 0.0, 1.0);
    out.color = input.color;
    out.border_color = input.border_color;
    out.pos = input.pos * globals.scale + snap;
    out.scale = input.scale * globals.scale;
    out.border_radius = border_radius * globals.scale;
    out.border_width = input.border_width * globals.scale;
    out.blur_radius = input.blur_radius * globals.scale;
    
    // Calculate UV coordinates for texture effects
    out.uv = vertex_pos;

    return out;
}

@fragment
fn frosted_glass_fs_main(input: BlurVertexOutput) -> @location(0) vec4<f32> {
    let frag_pos = input.position.xy;
    
    let border_radius = select_border_radius(
        input.border_radius,
        frag_pos,
        input.pos + input.scale * 0.5
    );

    // Calculate distance to quad edge for masking
    let dist_to_quad = distance_alg(
        frag_pos,
        input.pos,
        input.scale,
        border_radius
    );

    // Apply smooth alpha based on distance
    let alpha_mask = 1.0 - smoothstep(-0.5, 0.5, dist_to_quad);
    
    if alpha_mask <= 0.0 {
        discard;
    }

    // Handle border if present
    var mixed_color: vec4<f32> = input.color;
    if input.border_width > 0.0 {
        let internal_border = max(border_radius - input.border_width, 0.0);
        let internal_distance = distance_alg(
            frag_pos,
            input.pos + vec2<f32>(input.border_width, input.border_width),
            input.scale - vec2<f32>(input.border_width * 2.0, input.border_width * 2.0),
            internal_border
        );

        let border_mix = smoothstep(
            max(internal_border - 0.5, 0.0),
            internal_border + 0.5,
            internal_distance
        );

        mixed_color = mix(input.color, input.border_color, vec4<f32>(border_mix, border_mix, border_mix, border_mix));
    }

    // Apply frosted glass effect based on blur radius
    var final_color = mixed_color;
    if input.blur_radius > 0.0 {
        let blur_strength = min(input.blur_radius / 20.0, 1.0); // Normalize blur strength
        let frosted_effect = frosted_glass_effect(input.uv, blur_strength);
        
        // Blend the frosted glass effect with the base color
        let blended_rgb = mix(mixed_color.rgb, frosted_effect * mixed_color.rgb, blur_strength * 0.3);
        final_color = vec4<f32>(blended_rgb, mixed_color.a);
        
        // Add subtle brightness variation for glass-like appearance
        let brightness_variation = frosted_effect * 0.05;
        final_color = vec4<f32>(
            final_color.r + brightness_variation.r,
            final_color.g + brightness_variation.g,
            final_color.b + brightness_variation.b,
            final_color.a
        );
    }

    return vec4<f32>(final_color.rgb, final_color.a * alpha_mask);
}