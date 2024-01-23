struct Globals {
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var font_color_tex: texture_2d<f32>;
@group(0) @binding(2) var font_mask_tex: texture_2d<f32>;
@group(0) @binding(3) var font_sampler: sampler;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) v_pos: vec4<f32>,
    @location(1) v_color: vec4<f32>,
    @location(2) v_uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) f_color: vec4<f32>,
    @location(1) f_uv: vec2<f32>,
    @location(2) f_use_tex: i32,
    @location(3) f_use_mask: i32,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.f_color = input.v_color;
    out.f_uv = input.v_uv;

    var use_tex: i32 = 0;
    var use_mask: i32 = 0;
    var flags: i32 = i32(input.v_pos.w);
    if (flags == 1) {
        use_tex = 1;
    } else if (flags == 2) {
        use_mask = 1;
    } else if (flags == 3) {
        use_tex = 1;
        use_mask = 1;
    }

    out.f_use_tex = use_tex;
    out.f_use_mask = use_mask;
    out.position = globals.transform * vec4<f32>(input.v_pos.xy, 0.0, 1.0);
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // return vec4<f32>(input.f_color.xyz, 1.0);
    var out: vec4<f32> = input.f_color;

    if input.f_use_tex > 0 {
        out = textureSampleLevel(font_color_tex, font_sampler, input.f_uv, 0.0);
    }

    if input.f_use_mask > 0 {
        out = vec4<f32>(out.xyz, textureSampleLevel(font_mask_tex, font_sampler, input.f_uv, 0.0).x);
    }

    return out;
}