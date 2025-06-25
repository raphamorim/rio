enable f16;

struct Globals {
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var font_sampler: sampler;
@group(1) @binding(0) var color_texture: texture_2d<f32>; // RGBA texture for color glyphs
@group(1) @binding(1) var mask_texture: texture_2d<f32>;  // R8 texture for alpha masks

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) v_pos: vec4<f32>,
    @location(1) v_color: vec4<f32>,
    @location(2) v_uv: vec2<f32>,
    @location(3) layers: vec2<i32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) f_color: vec4<f16>,
    @location(1) f_uv: vec2<f16>,
    @location(2) color_layer: i32,
    @location(3) mask_layer: i32,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.f_color = vec4<f16>(input.v_color);
    out.f_uv = vec2<f16>(input.v_uv);
    out.color_layer = input.layers.x;
    out.mask_layer = input.layers.y;

    out.position = globals.transform * vec4<f32>(input.v_pos.xy, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    var out: vec4<f16> = input.f_color;

    if input.color_layer > 0 {
        let tex_sample = textureSampleLevel(color_texture, font_sampler, vec2<f32>(input.f_uv), 0.0);
        out = vec4<f16>(tex_sample);
    }

    if input.mask_layer > 0 {
        let tex_alpha = textureSampleLevel(mask_texture, font_sampler, vec2<f32>(input.f_uv), 0.0).x;
        out = vec4<f16>(out.xyz, input.f_color.a * f16(tex_alpha));
    }

    return vec4<f32>(out);
}
