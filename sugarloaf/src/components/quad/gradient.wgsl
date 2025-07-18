struct Globals {
    transform: mat4x4<f32>,
    scale: f32,
}

@group(0) @binding(0) var<uniform> globals: Globals;

fn vertex_position(vertex_index: u32) -> vec2<f32> {
    switch vertex_index {
        case 0u: { return vec2<f32>(-1.0, -1.0); }
        case 1u: { return vec2<f32>( 1.0, -1.0); }
        case 2u: { return vec2<f32>( 1.0,  1.0); }
        case 3u: { return vec2<f32>(-1.0, -1.0); }
        case 4u: { return vec2<f32>( 1.0,  1.0); }
        case 5u: { return vec2<f32>(-1.0,  1.0); }
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

fn premultiply(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

fn unpack_u32(data: vec2<u32>) -> vec4<f32> {
    let first = data.x;
    let second = data.y;
    
    let a = f32((first >> 16u) & 0xFFFFu) / 65535.0;
    let b = f32(first & 0xFFFFu) / 65535.0;
    let c = f32((second >> 16u) & 0xFFFFu) / 65535.0;
    let d = f32(second & 0xFFFFu) / 65535.0;
    
    return vec4<f32>(a, b, c, d);
}

fn unpack_color(data: vec2<u32>) -> vec4<f32> {
    return unpack_u32(data);
}

fn interpolate_color(start_color: vec4<f32>, end_color: vec4<f32>, factor: f32) -> vec4<f32> {
    return mix(start_color, end_color, factor);
}

fn rounded_box_sdf(to_center: vec2<f32>, size: vec2<f32>, radius: vec4<f32>) -> f32 {
    let r = select(
        select(radius.xy, radius.zw, to_center.x > 0.0),
        select(radius.xw, radius.yz, to_center.x > 0.0),
        to_center.y > 0.0
    );
    
    let q = abs(to_center) - size + r.x;
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r.x;
}

struct GradientVertexInput {
    @builtin(vertex_index) vertex_index: u32,
    @location(0) @interpolate(flat) colors_1: vec4<u32>,
    @location(1) @interpolate(flat) colors_2: vec4<u32>,
    @location(2) @interpolate(flat) offsets: vec4<u32>,
    @location(3) direction: vec4<f32>,
    @location(4) position_and_scale: vec4<f32>,
    @location(5) border_color: vec4<f32>,
    @location(6) border_radius: vec4<f32>,
    @location(7) border_width: f32,
    @location(8) snap: u32,
}

struct GradientVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(1) @interpolate(flat) colors_1: vec4<u32>,
    @location(2) @interpolate(flat) colors_2: vec4<u32>,
    @location(3) @interpolate(flat) offsets: vec4<u32>,
    @location(4) direction: vec4<f32>,
    @location(5) position_and_scale: vec4<f32>,
    @location(6) border_color: vec4<f32>,
    @location(7) border_radius: vec4<f32>,
    @location(8) border_width: f32,
}

@vertex
fn gradient_vs_main(input: GradientVertexInput) -> GradientVertexOutput {
    var out: GradientVertexOutput;

    var pos: vec2<f32> = input.position_and_scale.xy * globals.scale;
    var scale: vec2<f32> = input.position_and_scale.zw * globals.scale;

    var pos_snap = vec2<f32>(0.0, 0.0);
    var scale_snap = vec2<f32>(0.0, 0.0);

    if bool(input.snap) {
        pos_snap = round(pos + vec2(0.001, 0.001)) - pos;
        scale_snap = round(pos + scale + vec2(0.001, 0.001)) - pos - pos_snap - scale;
    }

    var min_border_radius = min(input.position_and_scale.z, input.position_and_scale.w) * 0.5;
    var border_radius: vec4<f32> = vec4<f32>(
        min(input.border_radius.x, min_border_radius),
        min(input.border_radius.y, min_border_radius),
        min(input.border_radius.z, min_border_radius),
        min(input.border_radius.w, min_border_radius)
    );

    var transform: mat4x4<f32> = mat4x4<f32>(
        vec4<f32>(scale.x + scale_snap.x + 1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, scale.y + scale_snap.y + 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(pos + pos_snap - vec2<f32>(0.5, 0.5), 0.0, 1.0)
    );

    out.position = globals.transform * transform * vec4<f32>(vertex_position(input.vertex_index), 0.0, 1.0);
    out.colors_1 = input.colors_1;
    out.colors_2 = input.colors_2;
    out.offsets = input.offsets;
    out.direction = input.direction * globals.scale;
    out.position_and_scale = vec4<f32>(pos + pos_snap, scale + scale_snap);
    out.border_color = premultiply(input.border_color);
    out.border_radius = border_radius * globals.scale;
    out.border_width = input.border_width * globals.scale;

    return out;
}

fn random(coords: vec2<f32>) -> f32 {
    return fract(sin(dot(coords, vec2(12.9898,78.233))) * 43758.5453);
}

/// Returns the current interpolated color with a max 8-stop gradient
fn gradient(
    raw_position: vec2<f32>,
    direction: vec4<f32>,
    colors: array<vec4<f32>, 8>,
    offsets: array<f32, 8>,
    last_index: i32
) -> vec4<f32> {
    let start = direction.xy;
    let end = direction.zw;

    let v1 = end - start;
    let v2 = raw_position - start;
    let unit = normalize(v1);
    let coord_offset = dot(unit, v2) / length(v1);

    //need to store these as a var to use dynamic indexing in a loop
    //this is already added to wgsl spec but not in wgpu yet
    var colors_arr = colors;
    var offsets_arr = offsets;

    var color: vec4<f32>;

    let noise_granularity: f32 = 0.3/255.0;

    for (var i: i32 = 0; i < last_index; i++) {
        let curr_offset = offsets_arr[i];
        let next_offset = offsets_arr[i+1];

        if (coord_offset <= offsets_arr[0]) {
            color = colors_arr[0];
        }

        if (curr_offset <= coord_offset && coord_offset <= next_offset) {
            let start_color = colors_arr[i];
            let end_color = colors_arr[i+1];
            let factor = smoothstep(curr_offset, next_offset, coord_offset);

            color = interpolate_color(start_color, end_color, factor);
        }

        if (coord_offset >= offsets_arr[last_index]) {
            color = colors_arr[last_index];
        }
    }

    return color + mix(-noise_granularity, noise_granularity, random(raw_position));
}

@fragment
fn gradient_fs_main(input: GradientVertexOutput) -> @location(0) vec4<f32> {
    let colors = array<vec4<f32>, 8>(
        unpack_color(vec2<u32>(input.colors_1.x, input.colors_1.y)),
        unpack_color(vec2<u32>(input.colors_1.z, input.colors_1.w)),
        unpack_color(vec2<u32>(input.colors_2.x, input.colors_2.y)),
        unpack_color(vec2<u32>(input.colors_2.z, input.colors_2.w)),
        vec4<f32>(0.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 0.0),
    );

    let offsets_1: vec4<f32> = unpack_u32(vec2<u32>(input.offsets.x, input.offsets.y));
    let offsets_2: vec4<f32> = unpack_u32(vec2<u32>(input.offsets.z, input.offsets.w));

    var offsets = array<f32, 8>(
        offsets_1.x,
        offsets_1.y,
        offsets_1.z,
        offsets_1.w,
        offsets_2.x,
        offsets_2.y,
        offsets_2.z,
        offsets_2.w,
    );

    //TODO could just pass this in to the shader but is probably more performant to just check it here
    var last_index = 7;
    for (var i: i32 = 0; i <= 7; i++) {
        if (offsets[i] > 1.0) {
            last_index = i - 1;
            break;
        }
    }

    var mixed_color: vec4<f32> = gradient(input.position.xy, input.direction, colors, offsets, last_index);

    let pos = input.position_and_scale.xy;
    let scale = input.position_and_scale.zw;

    var dist: f32 = rounded_box_sdf(
        -(input.position.xy - pos - scale / 2.0) * 2.0,
        scale,
        input.border_radius * 2.0
    ) / 2.0;

    if (input.border_width > 0.0) {
        mixed_color = mix(
            mixed_color,
            input.border_color,
            clamp(0.5 + dist + input.border_width, 0.0, 1.0)
        );
    }

    return mixed_color * clamp(0.5-dist, 0.0, 1.0);
}