// Backdrop blur quad shader for Sugarloaf
// Implements CSS backdrop-filter: blur() equivalent

struct Globals {
    transform: mat4x4<f32>,
    scale: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var background_texture: texture_2d<f32>;
@group(1) @binding(1) var background_sampler: sampler;

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

// Gaussian blur kernel weights for 9-tap blur
fn gaussian_weight(offset: f32, sigma: f32) -> f32 {
    let sigma_sq = sigma * sigma;
    return exp(-(offset * offset) / (2.0 * sigma_sq));
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
fn backdrop_blur_vs_main(input: BlurVertexInput) -> BlurVertexOutput {
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
    
    // Calculate UV coordinates for texture sampling
    out.uv = vertex_pos;

    return out;
}

@fragment
fn backdrop_blur_fs_main(input: BlurVertexOutput) -> @location(0) vec4<f32> {
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

    // Discard fragments outside the quad
    if dist_to_quad > 0.0 {
        discard;
    }

    // Sample and blur the background texture
    var blurred_color = vec4<f32>(0.0);
    var total_weight = 0.0;
    
    let texture_size = vec2<f32>(textureDimensions(background_texture));
    let pixel_size = 1.0 / texture_size;
    
    // Convert screen position to UV coordinates
    let screen_uv = frag_pos / texture_size;
    
    if input.blur_radius > 0.0 {
        // Apply Gaussian blur to background
        let blur_sigma = input.blur_radius / 3.0; // Convert radius to sigma
        let blur_samples = i32(min(input.blur_radius, 16.0)); // Limit samples for performance
        
        for (var x = -blur_samples; x <= blur_samples; x++) {
            for (var y = -blur_samples; y <= blur_samples; y++) {
                let offset = vec2<f32>(f32(x), f32(y));
                let sample_uv = screen_uv + offset * pixel_size;
                
                // Calculate Gaussian weight
                let distance = length(offset);
                let weight = gaussian_weight(distance, blur_sigma);
                
                // Sample the background texture
                let sample_color = textureSample(background_texture, background_sampler, sample_uv);
                blurred_color += sample_color * weight;
                total_weight += weight;
            }
        }
        
        blurred_color /= total_weight;
    } else {
        // No blur, just sample the background directly
        blurred_color = textureSample(background_texture, background_sampler, screen_uv);
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

    // Composite the blurred background with the quad color
    let final_color = mix(blurred_color, mixed_color, mixed_color.a);
    
    return vec4<f32>(final_color.rgb, 1.0);
}