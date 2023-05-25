struct Globals {
    transform: mat4x4<f32>,
    scale: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) vertex_position: vec2<f32>,
    @location(1) in_pos: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) size: vec2<f32>,
) -> VertexOutput {
    var output: VertexOutput;

    var pos: vec2<f32> = in_pos * globals.scale;
    var scale: vec2<f32> = size * globals.scale;

    var transform: mat4x4<f32> = mat4x4<f32>(
        vec4<f32>(scale.x + 1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, scale.y + 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(pos - vec2<f32>(0.5, 0.5), 0.0, 1.0)
    );

    output.color = color;
    output.position = globals.transform * transform * vec4<f32>(vertex_position, 0.0, 1.0);
    return output;
}

@fragment
fn fs_main(output: VertexOutput) -> @location(0) vec4<f32> {
    return output.color;
}
