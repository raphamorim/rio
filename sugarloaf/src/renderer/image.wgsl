// Image rendering shader — instanced, one instance per image placement.
// Matches Ghostty's approach: vertex_id generates quad corners,
// per-instance data provides position and source rect.

struct Globals {
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var image_sampler: sampler;
@group(1) @binding(0) var image_texture: texture_2d<f32>;

struct InstanceInput {
    // Screen position of the image top-left (physical pixels).
    @location(0) dest_pos: vec2<f32>,
    // Size of the image on screen (physical pixels).
    @location(1) dest_size: vec2<f32>,
    // Source rectangle in the texture: xy = origin, zw = size (normalized 0..1).
    @location(2) source_rect: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vid: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Triangle strip: 4 vertices → quad
    //   0 → 1
    //   |  /|
    //   2 → 3
    var corner: vec2<f32>;
    corner.x = f32(vid == 1u || vid == 3u);
    corner.y = f32(vid == 2u || vid == 3u);

    // Texture coordinates from source rect
    var tex_coord = instance.source_rect.xy + instance.source_rect.zw * corner;

    // Screen position
    var image_pos = instance.dest_pos + instance.dest_size * corner;

    var out: VertexOutput;
    out.position = globals.transform * vec4<f32>(image_pos, 0.0, 1.0);
    out.tex_coord = tex_coord;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var rgba = textureSampleLevel(image_texture, image_sampler, input.tex_coord, 0.0);
    // Premultiply alpha
    rgba = vec4<f32>(rgba.rgb * rgba.a, rgba.a);
    return rgba;
}
