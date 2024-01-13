//! Shader code for rendering batches.

/// OpenGL shader code.
pub mod gl {
    /// Base vertex shader.
    pub const BASE_VS: &str = r#"
#version 410
layout(location = 0) in vec4 v_pos;
layout(location = 1) in vec4 v_color;
layout(location = 2) in vec2 v_uv;
layout(location = 0) out vec4 color;
layout(location = 1) out vec2 uv;
layout(location = 2) out float use_tex;
layout(location = 3) out float use_mask;
uniform mat4 view_proj;
void main() {
    uv = v_uv;
    color = v_color;
    use_tex = 0.0;
    use_mask = 0.0;
    int flags = int(v_pos.w);
    if (flags == 1) {
        use_tex = 1.0;
    } else if (flags == 2) {
        use_mask = 1.0;
    } else if (flags == 3) {
        use_tex = 1.0;
        use_mask = 1.0;
    }
    gl_Position = vec4(v_pos.xyz, 1.0) * view_proj;
}
"#;

    /// Base fragment shader.
    pub const BASE_FS: &str = r#"
#version 410
out vec4 frag_color;
layout(location = 0) in vec4 color;
layout(location = 1) in vec2 uv;
layout(location = 2) in float use_tex;
layout(location = 3) in float use_mask;
uniform sampler2D tex;
uniform sampler2D mask;
void main() {
    vec4 frag = color;    
    if (use_tex > 0.0) {
        frag *= texture(tex, uv);
    }
    if (use_mask > 0.0) {
        frag.a *= texture(mask, uv).a;
    }
    frag_color = frag;
}
"#;

    /// Subpixel text fragment shader.
    pub const SUBPIXEL_FS: &str = r#"
#version 410
layout(location = 0, index = 0) out vec4 frag_color;
layout(location = 0, index = 1) out vec4 frag_alpha;
layout(location = 0) in vec4 color;
layout(location = 1) in vec2 uv;
layout(location = 2) in float use_tex;
layout(location = 3) in float use_mask;
uniform sampler2D tex;
uniform sampler2D mask;
const float gamma_lut[256] = float[256](
    0.000, 0.058, 0.117, 0.175, 0.234, 0.293, 0.353, 0.413, 0.473, 0.534, 0.597, 0.661, 0.727, 0.797, 0.876, 1.000, 
    0.000, 0.021, 0.082, 0.143, 0.203, 0.264, 0.325, 0.386, 0.448, 0.510, 0.572, 0.635, 0.700, 0.766, 0.836, 1.000, 
    0.000, 0.000, 0.034, 0.098, 0.161, 0.224, 0.287, 0.350, 0.413, 0.475, 0.538, 0.601, 0.665, 0.729, 0.793, 1.000, 
    0.000, 0.000, 0.000, 0.033, 0.099, 0.165, 0.231, 0.296, 0.360, 0.425, 0.489, 0.552, 0.616, 0.679, 0.741, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000, 
    0.000, 0.092, 0.180, 0.264, 0.345, 0.422, 0.496, 0.566, 0.633, 0.696, 0.756, 0.812, 0.864, 0.913, 0.958, 1.000 
);

float luma(vec4 color) {
    return color.x * 0.25 + color.y * 0.72 + color.z * 0.075;
}

float gamma_alpha(float luma, float alpha) {
    int luma_index = int(clamp(luma * 15.0, 0.0, 15.0));
    int alpha_index = int(clamp(alpha * 15.0, 0.0, 15.0));
    return gamma_lut[luma_index * 15 + alpha_index];
}

vec4 subpx_gamma_alpha(vec4 color, vec4 mask) {
    float l = luma(color);
    return vec4(
        gamma_alpha(l, mask.x * color.a),
        gamma_alpha(l, mask.y * color.a),
        gamma_alpha(l, mask.z * color.a),
        1.0
    );
}

const float GAMMA = 1.0 / 1.2;
const float CONTRAST = 0.8;

float gamma_correct(float luma, float alpha, float gamma, float contrast) {
    float inverse_luma = 1.0 - luma;
    float inverse_alpha = 1.0 - alpha;
    float g = pow(luma * alpha + inverse_luma * inverse_alpha, gamma);
    float a = (g - inverse_luma) / (luma - inverse_luma);
    a = a + ((1.0 - a) * contrast * a);
    return clamp(a, 0.0, 1.0);
}

vec4 gamma_correct_subpx(vec4 color, vec4 mask) {
    float l = luma(color);
    float inverse_luma = 1.0 - l;
    float gamma = mix(1.0 / 1.2, 1.0 / 2.4, inverse_luma);
    float contrast = mix(0.1, 0.8, inverse_luma);
    return vec4(
        gamma_correct(l, mask.x * color.a, gamma, contrast),
        gamma_correct(l, mask.y * color.a, gamma, contrast),
        gamma_correct(l, mask.z * color.a, gamma, contrast),
        1.0
    );
}

void main() {
    vec4 frag = color;
    vec4 alpha = texture(mask, uv);
    frag_color = vec4(color.xyz, 1.0);
    //frag_alpha = subpx_gamma_alpha(frag, alpha);
    frag_alpha = gamma_correct_subpx(frag, alpha);
    //  frag_alpha = vec4(alpha.xyz * color.a, 1.0);
    // frag.a *= (alpha.r + alpha.g + alpha.b) * 0.3333; 
    // frag_color = frag;
    //frag_color = vec4(alpha.xyz, 1.0); //frag;
}
"#;
}
