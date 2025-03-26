use librashader_presets::{ParsePresetError, ShaderFeatures, ShaderPreset};
use std::fs::{File, Path};

pub fn shader_preset() -> Result<ShaderPreset, ParsePresetError> {
    // TODO: Create folder in /tpm or /config folder
    let tmp = std::env::temp_dir();
    let filter_path = Path::new(format!("{}/newpixiecrt", tmp.to_string_lossy()));

    if let Ok(created_path) = std::fs::create_dir_all(filter_path) {}

    ShaderPreset::try_parse("./slangp/newpixie-crt.slangp", ShaderFeatures::NONE)
}

fn create_accumulate(folder: Path) {
    let file_path = Path::new(format!("{}/accumulate.slang", folder));
    let mut file = File::create("accumulate.slang").unwrap();
    file.write_all(
        r#"
        #version 450

        layout(push_constant) uniform Push
        {
	vec4 SourceSize;
	vec4 OriginalSize;
	vec4 OutputSize;
	uint FrameCount;
	float acc_modulate;
        } params;

        #pragma parameter acc_modulate "Accumulate Modulation" 0.65 0.0 1.0 0.01
        #define modulate params.acc_modulate

        #define tex0 PassFeedback1
        #define tex1 Source

        layout(std140, set = 0, binding = 0) uniform UBO
        {
	mat4 MVP;
        } global;

        #pragma stage vertex
        layout(location = 0) in vec4 Position;
        layout(location = 1) in vec2 TexCoord;
        layout(location = 0) out vec2 vTexCoord;

        void main()
        {
           gl_Position = global.MVP * Position;
           vTexCoord = TexCoord;
        }

        #pragma stage fragment
        layout(location = 0) in vec2 vTexCoord;
        layout(location = 0) out vec4 FragColor;
        layout(set = 0, binding = 2) uniform sampler2D Source;
        layout(set = 0, binding = 3) uniform sampler2D PassFeedback1;

        void main()
        {
           vec4 a = texture(tex0, vTexCoord.xy) * vec4(modulate);
           vec4 b = texture(tex1, vTexCoord.xy);
           FragColor = max( a, b * 0.96 );
        }"#,
    )?;
}
