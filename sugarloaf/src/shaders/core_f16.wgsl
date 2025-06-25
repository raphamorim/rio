// Unified shader for Rio Terminal - Single Point of Definition (SPD) - F16 Version
// Handles all rendering types: quads (solid/textured) and text
// render_mode: 0.0 = quad, 1.0 = text
// For quads: extended.w = 0.0 (solid) or 1.0 (textured)

enable f16;

struct Globals {
    transform: mat4x4<f32>,
    scale: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var main_sampler: sampler;
@group(1) @binding(0) var main_texture: texture_2d_array<f32>; // f16 textures are sampled as f32

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) position: vec4<f32>,     // x, y, z, render_mode
    @location(1) color: vec4<f32>,        // r, g, b, a
    @location(2) uv_layer: vec4<f32>,     // u, v, layer, mask_layer
    @location(3) size_border: vec4<f32>,  // width, height, border_width, border_radius
    @location(4) extended: vec4<f32>,     // shadow_blur, shadow_offset_x, shadow_offset_y, texture_flag
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f16>,
    @location(1) uv: vec2<f16>,
    @location(2) layer: f32,
    @location(3) mask_layer: f32,
    @location(4) size: vec2<f16>,
    @location(5) border_width: f16,
    @location(6) border_radius: f16,
    @location(7) shadow_blur: f16,
    @location(8) shadow_offset: vec2<f16>,
    @location(9) render_mode: f32,
    @location(10) world_pos: vec2<f16>,
    @location(11) texture_flag: f32,
}

// Compute the normalized quad coordinates based on the vertex index
fn vertex_position(vertex_index: u32) -> vec2<f32> {
    return vec2<f32>((vec2(1u, 2u) + vertex_index) % vec2(6u) < vec2(3u));
}

// Given a vector from a point to the center of a rounded rectangle
fn rounded_box_sdf(to_center: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    return length(max(abs(to_center) - size + vec2<f32>(radius, radius), vec2<f32>(0.0, 0.0))) - radius;
}

// Select border radius based on fragment position
fn select_border_radius(radii: vec4<f32>, position: vec2<f32>, center: vec2<f32>) -> f32 {
    var rx = radii.x;
    var ry = radii.y;
    rx = select(radii.x, radii.y, position.x > center.x);
    ry = select(radii.w, radii.z, position.x > center.x);
    rx = select(rx, ry, position.y > center.y);
    return rx;
}

fn distance_alg(frag_coord: vec2<f32>, position: vec2<f32>, size: vec2<f32>, radius: f32) -> f32 {
    var inner_half_size: vec2<f16> = vec2<f16>((size - vec2<f32>(radius, radius) * 2.0) / 2.0);
    var top_left: vec2<f16> = vec2<f16>(position + vec2<f32>(radius, radius));
    return rounded_box_sdf(frag_coord - vec2<f32>(top_left + inner_half_size), vec2<f32>(inner_half_size), 0.0);
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    
    let render_mode = input.position.w;
    let vertex_pos = vertex_position(input.vertex_index);
    
    // Common setup
    out.color = vec4<f16>(input.color);
    out.uv = vec2<f16>(input.uv_layer.xy);
    out.layer = input.uv_layer.z;
    out.mask_layer = input.uv_layer.w;
    out.size = vec2<f16>(input.size_border.xy);
    out.border_width = f16(input.size_border.z);
    out.border_radius = f16(input.size_border.w);
    out.shadow_blur = f16(input.extended.x);
    out.shadow_offset = vec2<f16>(input.extended.yz);
    out.texture_flag = input.extended.w;
    out.render_mode = render_mode;
    
    var world_position: vec2<f32>;
    
    if render_mode == 0.0 {
        // Quad rendering (solid or textured)
        var pos: vec2<f32> = (input.position.xy + min(input.extended.yz, vec2<f32>(0.0, 0.0)) - input.extended.x) * globals.scale;
        var scale: vec2<f32> = (input.size_border.xy + vec2<f32>(abs(input.extended.y), abs(input.extended.z)) + input.extended.x * 2.0) * globals.scale;
        var snap: vec2<f32> = vec2<f32>(0.0, 0.0);

        if input.size_border.x == 1.0 {
            snap.x = round(pos.x) - pos.x;
        }
        if input.size_border.y == 1.0 {
            snap.y = round(pos.y) - pos.y;
        }

        var transform: mat4x4<f32> = mat4x4<f32>(
            vec4<f32>(scale.x + 1.0, 0.0, 0.0, 0.0),
            vec4<f32>(0.0, scale.y + 1.0, 0.0, 0.0),
            vec4<f32>(0.0, 0.0, 1.0, 0.0),
            vec4<f32>(pos - vec2<f32>(0.5, 0.5) + snap, 0.0, 1.0)
        );

        world_position = input.position.xy * globals.scale + snap;
        out.position = globals.transform * transform * vec4<f32>(vertex_pos, 0.0, 1.0);
        
        // For textured quads, set up UV coordinates properly
        if input.extended.w > 0.0 {
            // Textured quad - use vertex position as UV base and apply atlas scaling
            out.uv = vec2<f16>(vertex_pos * input.uv_layer.zw + input.uv_layer.xy);
        }
        
    } else if render_mode == 1.0 {
        // Text rendering
        world_position = input.position.xy;
        out.position = globals.transform * vec4<f32>(input.position.xy, 0.0, 1.0);
    }
    
    out.world_pos = vec2<f16>(world_position);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let render_mode = input.render_mode;
    
    if render_mode == 0.0 {
        // Quad rendering (solid or textured)
        var base_color: vec4<f16>;
        
        if input.texture_flag > 0.0 {
            // Textured quad - sample from texture
            let tex_sample = textureSample(main_texture, main_sampler, vec2<f32>(input.uv), i32(input.layer));
            base_color = vec4<f16>(tex_sample);
            // Modulate with vertex color for tinting
            base_color = base_color * input.color;
        } else {
            // Solid quad - use vertex color
            base_color = input.color;
        }
        
        var mixed_color: vec4<f16> = base_color;
        let border_radius = f32(input.border_radius);
        
        // Apply border effects (works for both solid and textured quads)
        if f32(input.border_width) > 0.0 {
            let internal_border = max(border_radius - f32(input.border_width), 0.0);
            let internal_distance = distance_alg(
                input.position.xy,
                vec2<f32>(input.world_pos) + vec2<f32>(f32(input.border_width)),
                vec2<f32>(input.size) - vec2<f32>(f32(input.border_width) * 2.0),
                internal_border
            );
            
            let border_mix = f16(smoothstep(
                max(internal_border - 0.5, 0.0),
                internal_border + 0.5,
                internal_distance
            ));
            
            // Border color (darker version of base color)
            let border_color = vec4<f16>(base_color.xyz * f16(0.8), base_color.w);
            mixed_color = mix(base_color, border_color, vec4<f16>(border_mix));
        }
        
        // Apply rounded corners (works for both solid and textured quads)
        let dist = distance_alg(
            input.position.xy,
            vec2<f32>(input.world_pos),
            vec2<f32>(input.size),
            border_radius
        );
        
        let radius_alpha = f16(1.0 - smoothstep(
            max(border_radius - 0.5, 0.0),
            border_radius + 0.5,
            dist
        ));
        
        return vec4<f32>(mixed_color.xyz, mixed_color.w * radius_alpha);
        
    } else if render_mode == 1.0 {
        // Text rendering
        var out: vec4<f16> = input.color;
        
        if input.layer > 0.0 {
            let tex_sample = textureSampleLevel(main_texture, main_sampler, vec2<f32>(input.uv), i32(input.layer), 0.0);
            out = vec4<f16>(tex_sample);
        }
        
        if input.mask_layer > 0.0 {
            let tex_alpha = textureSampleLevel(main_texture, main_sampler, vec2<f32>(input.uv), i32(input.mask_layer), 0.0).x;
            out = vec4<f16>(out.xyz, input.color.a * f16(tex_alpha));
        }
        
        return vec4<f32>(out);
    }
    
    // Fallback
    return vec4<f32>(input.color);
}