struct VertexInput {
    @location(0) v_pos: vec2<f32>,
    @location(1) pos: vec2<f32>,
    @builtin(vertex_index) vertex_index: u32,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
}

@vertex
fn vs_main(
    @location(0) v_pos: vec4<f32>,
    @location(1) pos: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;

    //let x = f32(1 - i32(input.vertex_index)) * 0.5;
    //let y = f32(1 - i32(input.vertex_index & 1u) * 2) * 0.5;

    //out.pos = vec4<f32>(x, y, 0.0, 1.0);
    // var pos: vec2<f32> = vec2<f32>(0.0, 0.0);
    out.pos = vec4<f32>(pos, 0.0, 1.0);

    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 0.0, 1.0);
}