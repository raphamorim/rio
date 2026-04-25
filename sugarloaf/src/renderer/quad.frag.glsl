#version 450

// Quad fragment shader — port of `fs_main` from
// `sugarloaf/src/renderer/renderer.metal`. Implements:
//   * scissor-style clipping via `clip_rect`
//   * underline pattern rasterization (regular / dashed / dotted /
//     curly) — used by rich-text decorations and terminal cell
//     underlines that emit through the rich-text quad path.
//   * SDF rounded-corner mask (with edge antialiasing)
//   * sRGB → output colorspace transform on the fill color
//
// NOT ported (yet):
//   * glyph atlas sampling (color_layer / mask_layer paths) — the
//     grid text pass and UI text overlay each own a dedicated
//     atlas-sampling pipeline; the rich-text path doesn't render
//     terminal glyphs.
//
// Fragments with `color_layer > 0` or `mask_layer > 0` (rich-text
// glyph instances that slip through here) fall back to the solid-
// fill path — graceful degradation rather than a visual crash.

layout(set = 0, binding = 0, std140) uniform Globals {
    mat4 transform;
    uint input_colorspace;
    uint _pad0;
    uint _pad1;
    uint _pad2;
} globals;

layout(location = 0)      in vec4 in_color;
layout(location = 1)      in vec2 in_uv;
layout(location = 2) flat in ivec2 in_layers;
layout(location = 3)      in vec4 in_corner_radii;
layout(location = 4)      in vec2 in_rect_size;
layout(location = 5) flat in int  in_underline_style;
layout(location = 6) flat in vec4 in_clip_rect;

layout(location = 0) out vec4 out_color;

// ----- Colorspace helpers (mirror grid_bg.frag.glsl) -----
vec3 srgb_to_linear(vec3 c) {
    vec3 lo = c / 12.92;
    vec3 hi = pow((c + 0.055) / 1.055, vec3(2.4));
    return mix(lo, hi, greaterThan(c, vec3(0.04045)));
}
vec3 linear_to_srgb(vec3 c) {
    vec3 lo = c * 12.92;
    vec3 hi = pow(c, vec3(1.0 / 2.4)) * 1.055 - 0.055;
    return mix(lo, hi, greaterThan(c, vec3(0.0031308)));
}
vec3 srgb_to_p3(vec3 linear_srgb) {
    return vec3(
        dot(linear_srgb, vec3(0.82246197, 0.17753803, 0.0)),
        dot(linear_srgb, vec3(0.03319420, 0.96680580, 0.0)),
        dot(linear_srgb, vec3(0.01708263, 0.07239744, 0.91051993))
    );
}
vec3 rec2020_to_p3(vec3 linear_r2020) {
    return vec3(
        dot(linear_r2020, vec3( 1.34357825, -0.28217967, -0.06139858)),
        dot(linear_r2020, vec3(-0.06529745,  1.08782226, -0.02252481)),
        dot(linear_r2020, vec3( 0.00282179, -0.02598807,  1.02316628))
    );
}
vec3 prepare_output_rgb(vec3 srgb, uint cs) {
    vec3 lin = srgb_to_linear(srgb);
    if (cs == 0u) {
        lin = srgb_to_p3(lin);
    } else if (cs == 2u) {
        lin = rec2020_to_p3(lin);
    }
    return linear_to_srgb(lin);
}

// ----- SDF rounded-rect helpers (mirror renderer.metal) -----
float pick_corner_radius(vec2 center_to_point, vec4 corner_radii) {
    if (center_to_point.x < 0.0) {
        if (center_to_point.y < 0.0) {
            return corner_radii.x; // tl
        } else {
            return corner_radii.w; // bl
        }
    } else {
        if (center_to_point.y < 0.0) {
            return corner_radii.y; // tr
        } else {
            return corner_radii.z; // br
        }
    }
}

float quad_sdf(vec2 corner_center_to_point, float corner_radius) {
    if (corner_radius == 0.0) {
        return max(corner_center_to_point.x, corner_center_to_point.y);
    }
    float signed_distance_to_inset_quad =
        length(max(vec2(0.0), corner_center_to_point))
        + min(0.0, max(corner_center_to_point.x, corner_center_to_point.y));
    return signed_distance_to_inset_quad - corner_radius;
}

const float PI_F = 3.1415926;

// Modulus that has the same sign as `a` (mirrors `fmod_pos` in
// renderer.metal — GLSL's `mod()` rounds toward negative infinity
// which is *not* what we want here).
float fmod_pos(float a, float b) {
    return a - b * trunc(a / b);
}

// Underline pattern alpha. Mirrors `underline_alpha` in
// renderer.metal. `x_pos`/`y_pos` are in pixels relative to the
// underline rect's top-left, `rect_height` is the rect's height in
// pixels, `thickness` is the line thickness encoded into
// `corner_radii.x` by the CPU emit code. `style` matches the
// `underline_style` enum from `batch.rs`:
//   1 = regular (solid), 2 = dashed, 3 = dotted, 4 = curly
float underline_alpha(float x_pos, float y_pos, float rect_height,
                      float thickness, int style) {
    if (style == 1) {
        return 1.0;
    }
    if (style == 2) {
        // Dashed: 6px dash, 2px gap.
        const float antialias = 0.5;
        const float dash_width = 6.0;
        const float gap_width = 2.0;
        float period = dash_width + gap_width;
        float pos_in_period = fmod_pos(x_pos, period);
        float start_aa = clamp(pos_in_period / antialias, 0.0, 1.0);
        float end_aa = clamp((dash_width - pos_in_period) / antialias, 0.0, 1.0);
        return min(start_aa, end_aa);
    }
    if (style == 3) {
        // Dotted: 2px dot, 2px gap.
        const float antialias = 0.5;
        const float dot_width = 2.0;
        const float gap_width = 2.0;
        float period = dot_width + gap_width;
        float pos_in_period = fmod_pos(x_pos, period);
        float start_aa = clamp(pos_in_period / antialias, 0.0, 1.0);
        float end_aa = clamp((dot_width - pos_in_period) / antialias, 0.0, 1.0);
        return min(start_aa, end_aa);
    }
    if (style == 4) {
        // Curly (sine wave) via SDF. Same constants as renderer.metal.
        const float WAVE_FREQUENCY = 2.0;
        const float WAVE_HEIGHT_RATIO = 0.8;

        float half_thickness = thickness * 0.5;
        vec2 st = vec2(x_pos / rect_height, y_pos / rect_height - 0.5);
        float frequency = PI_F * WAVE_FREQUENCY * thickness / rect_height;
        float amplitude = (thickness * WAVE_HEIGHT_RATIO) / rect_height;

        float sine = sin(st.x * frequency) * amplitude;
        float dSine = cos(st.x * frequency) * amplitude * frequency;
        float dist = (st.y - sine) / sqrt(1.0 + dSine * dSine);
        float distance_in_pixels = dist * rect_height;
        float distance_from_top_border = distance_in_pixels - half_thickness;
        float distance_from_bottom_border = distance_in_pixels + half_thickness;
        return clamp(0.5 - max(-distance_from_bottom_border, distance_from_top_border),
                     0.0, 1.0);
    }
    return 1.0;
}

void main() {
    // Scissor-style clip rect.
    if (in_clip_rect.z > 0.0) {
        float px = gl_FragCoord.x;
        float py = gl_FragCoord.y;
        if (px < in_clip_rect.x
            || px >= in_clip_rect.x + in_clip_rect.z
            || py < in_clip_rect.y
            || py >= in_clip_rect.y + in_clip_rect.w)
        {
            discard;
        }
    }

    vec4 color = in_color;

    // Underline patterns are emitted as their own quad instances
    // (see `Compositor::push_underline` in `renderer/batch.rs`):
    //   color = underline color
    //   rect_size = (width, line_height) in pixels
    //   corner_radii.x = thickness
    //   underline_style ∈ {1, 2, 3, 4}
    // The fragment computes the per-pattern alpha and returns the
    // colorspace-transformed color * alpha. No SDF / corner masking
    // applied to underline rects — they're flat strips.
    if (in_underline_style > 0) {
        float width = in_rect_size.x;
        float rect_height = in_rect_size.y;
        float x_pos = in_uv.x * width;
        float y_pos = in_uv.y * rect_height;
        float thickness = in_corner_radii.x;
        float a = underline_alpha(x_pos, y_pos, rect_height, thickness,
                                  in_underline_style);
        out_color = vec4(
            prepare_output_rgb(color.rgb, globals.input_colorspace),
            color.a * a
        );
        return;
    }

    bool has_corners = in_corner_radii.x != 0.0
        || in_corner_radii.y != 0.0
        || in_corner_radii.z != 0.0
        || in_corner_radii.w != 0.0;

    // Fast path: sharp corners, no SDF.
    if (!has_corners) {
        out_color = vec4(prepare_output_rgb(color.rgb, globals.input_colorspace), color.a);
        return;
    }

    vec2 size = in_rect_size;
    vec2 half_size = size / 2.0;
    vec2 center_to_point = (in_uv - 0.5) * size;
    float corner_radius = pick_corner_radius(center_to_point, in_corner_radii);
    vec2 corner_to_point = abs(center_to_point) - half_size;
    vec2 corner_center_to_point = corner_to_point + corner_radius;
    float outer_sdf = quad_sdf(corner_center_to_point, corner_radius);

    const float antialias_threshold = 0.5;
    if (outer_sdf >= antialias_threshold) {
        discard;
    }
    float edge = clamp(antialias_threshold - outer_sdf, 0.0, 1.0);
    out_color = vec4(prepare_output_rgb(color.rgb, globals.input_colorspace), color.a * edge);
}
