use librashader_presets::{ParsePresetError, ShaderFeatures, ShaderPreset};

pub fn shader_preset() -> Result<ShaderPreset, ParsePresetError> {
    // TODO: Create folder in /tpm
    let tmp = std::env::temp_dir();
    let std::path::Path::new(format!("{}/newpixiecrt", tmp));

    ShaderPreset::try_parse("./slangp/newpixie-crt.slangp", ShaderFeatures::NONE)
}
