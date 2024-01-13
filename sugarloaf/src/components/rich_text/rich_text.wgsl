struct Globals {
    transform: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> globals: Globals;
@group(0) @binding(1) var<uniform> tex: texture_2d<f32>;
@group(0) @binding(2) var<uniform> mask: texture_2d<f32>;
// @group(0) @binding(2) var<uniform> mask: sampler;

struct VertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) v_pos: vec4<f32>,
    @location(1) v_color: vec4<f32>,
    @location(2) v_uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) use_tex: f32,
    @location(3) use_mask: f32,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.color = input.v_color;
    out.uv = input.v_uv;

    var use_tex: f32 = 0.0;
    var use_mask: f32 = 0.0;
    var flags: i32 = i32(v_pos.w);
    if (flags == 1) {
        use_tex = 1.0;
    } else if (flags == 2) {
        use_mask = 1.0;
    } else if (flags == 3) {
        use_tex = 1.0;
        use_mask = 1.0;
    }
    out.use_tex = use_tex;
    out.use_mask = use_mask;
    out.position = vec4<f32>(input.xyz, 1.0) * globals.transform;
    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    switch input.use_tex {
        case 0u: {
            return textureSampleLevel(tex, input.uv);
        }
        // if (use_mask > 0.0) {
        // frag.a *= texture(mask, uv).a;
        // }
        case 1u: {
            return input.color;
        }
        default: {
            return input.color;
        }
    }
}