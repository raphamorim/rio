struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) color: vec4<f16>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) color: vec4<f16>,
}

@group(0) @binding(0)
var<uniform> transform: mat4x4<f32>;

@group(0) @binding(1)
var glyph_sampler: sampler;

@group(1) @binding(0)
var glyph_texture: texture_2d<f32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.clip_position = transform * vec4<f32>(input.position, 0.0, 1.0);
    output.tex_coords = input.tex_coords;
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f16> {
    let alpha = textureSample(glyph_texture, glyph_sampler, input.tex_coords).r;
    return vec4<f16>(input.color.rgb, input.color.a * f16(alpha));
}