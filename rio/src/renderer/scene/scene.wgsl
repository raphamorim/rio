struct VertexInput {
    @location(0) pos: vec2<f32>,
    @builtin(vertex_index) in_vertex_index: u32
}

struct VertexOutput {
    @builtin(position) out_pos: vec4<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let x = f32(1 - i32(input.in_vertex_index)) * 0.5;
    let y = f32(1 - i32(input.in_vertex_index & 1u) * 2) * 0.5;

    out.out_pos = vec4<f32>(x, y, 0.0, 1.0);

    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 1.0, 0.0, 1.0);
}