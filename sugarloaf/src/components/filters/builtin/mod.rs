use librashader_presets::{ParsePresetError, ShaderFeatures, ShaderPreset};
use std::path::Path;
use std::fs;
use std::io::Write;

macro_rules! resource {
    ($resource:literal) => {
        include_bytes!($resource) as &[u8]
    };
}

use std::error::Error;
use std::fmt;

#[derive(Debug)]
pub enum LoadError {
    ParseError(ParsePresetError),
    IoError(std::io::Error)
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LoadError::ParseError(details) => {
                write!(f, "Parse failed: {}", details)
            }
            LoadError::IoError(details) => {
                write!(f, "File failed: {}", details)
            }
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            LoadError::IoError(err) => Some(err),
            _ => None
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(err: std::io::Error) -> Self {
        LoadError::IoError(err)
    }
}

const NEWPIXIECRT_ACCUMULATE: &[u8] = resource!("./newpixiecrt/accumulate.slang");
const NEWPIXIECRT_BLUR_HORIZ: &[u8] = resource!("./newpixiecrt/blur_horiz.slang");
const NEWPIXIECRT_BLUR_VERT: &[u8] = resource!("./newpixiecrt/blur_vert.slang");
const NEWPIXIECRT_CRTFRAME: &[u8] = resource!("./newpixiecrt/crtframe.png");
const NEWPIXIECRT_NEWPIXIECRT: &[u8] = resource!("./newpixiecrt/newpixie-crt.slang");
const NEWPIXIECRT_NEWPIXIECRTP: &[u8] = resource!("./newpixiecrt/newpixie-crt.slangp");

pub fn newpixiecrt() -> Result<ShaderPreset, LoadError> {
    let dir_path = Path::new("/tmp/newpixiecrt");
    if !dir_path.exists() {
        fs::create_dir_all(dir_path)?;
    }

    let files = vec![
        ("accumulate.slang", NEWPIXIECRT_ACCUMULATE),
        ("blur_horiz.slang", NEWPIXIECRT_BLUR_HORIZ),
        ("blur_vert.slang", NEWPIXIECRT_BLUR_VERT),
        ("crtframe.png", NEWPIXIECRT_CRTFRAME),
        ("newpixie-crt.slang", NEWPIXIECRT_NEWPIXIECRT),
        ("newpixie-crt.slangp", NEWPIXIECRT_NEWPIXIECRTP),
    ];

    // Create files in the directory
    for (filename, content) in files {
        let file_path = dir_path.join(filename);
        let mut file = fs::File::create(file_path)?;
        file.write_all(content)?;
    }

    match ShaderPreset::try_parse(
        dir_path.join("newpixie-crt.slangp"),
        ShaderFeatures::NONE,
    ) {
        Ok(preset) => Ok(preset),
        Err(err) => Err(LoadError::ParseError(err)),
    }
}

const FUBAXVR_CHROMATIC: &[u8] = resource!("./fubax_vr/Chromatic.slang");
const FUBAXVR_FILMIC_SHARPEN: &[u8] = resource!("./fubax_vr/FilmicSharpen.slang");
const FUBAXVR_FUBAXVRP: &[u8] = resource!("./fubax_vr/fubax_vr.slangp");
const FUBAXVR_FUBAXVR_PARAMS: &[u8] = resource!("./fubax_vr/fubax_vr_params.inc");
const FUBAXVR_FUBAXVR_SHARED_FUNCS: &[u8] = resource!("./fubax_vr/fubax_vr_shared_funcs.inc");
const FUBAXVR_NOSE: &[u8] = resource!("./fubax_vr/nose.png");
const FUBAXVR_STOCK: &[u8] = resource!("./fubax_vr/stock.slang");
const FUBAXVR_VR: &[u8] = resource!("./fubax_vr/VR.slang");
const FUBAXVR_VR_NOSE: &[u8] = resource!("./fubax_vr/VR_nose.slang");

pub fn fubaxvr() -> Result<ShaderPreset, LoadError> {
    let dir_path = Path::new("/tmp/fubax_vr");
    if !dir_path.exists() {
        fs::create_dir_all(dir_path)?;
    }

    let files = vec![
        ("Chromatic.slang",  FUBAXVR_CHROMATIC),
        ("FilmicSharpen.slang",  FUBAXVR_FILMIC_SHARPEN),
        ("fubax_vr.slangp", FUBAXVR_FUBAXVRP),
        ("fubax_vr_params.inc",  FUBAXVR_FUBAXVR_PARAMS),
        ("fubax_vr_shared_funcs.inc",  FUBAXVR_FUBAXVR_SHARED_FUNCS),
        ("nose.png",  FUBAXVR_NOSE),
        ("stock.slang",  FUBAXVR_STOCK),
        ("VR.slang",  FUBAXVR_VR),
        ("VR_nose.slang", FUBAXVR_VR_NOSE),
    ];

    // Create files in the directory
    for (filename, content) in files {
        let file_path = dir_path.join(filename);
        let mut file = fs::File::create(file_path)?;
        file.write_all(content)?;
    }

    match ShaderPreset::try_parse(
        dir_path.join("fubax_vr.slangp"),
        ShaderFeatures::NONE,
    ) {
        Ok(preset) => Ok(preset),
        Err(err) => Err(LoadError::ParseError(err)),
    }
}

const NTSC_VR_IMAGE_ADJUSTMENT: &[u8] = resource!("./ntsc_vcr/image-adjustment.slang");
const NTSC_VR_IMAGE_ADJUSTMENTP: &[u8] = resource!("./ntsc_vcr/image-adjustment.slangp");
const NTSC_VR_NTSC_PASS1_COMPOSITE_3PHASE: &[u8] = resource!("./ntsc_vcr/ntsc-pass1-composite-3phase.slang");
const NTSC_VR_NTSC_PASS2_3PHASE: &[u8] = resource!("./ntsc_vcr/ntsc-pass2-3phase.slang");
const NTSC_VR_STOCK: &[u8] = resource!("./ntsc_vcr/ntsc-stock.slang");
const NTSC_VR_VCR: &[u8] = resource!("./ntsc_vcr/ntsc-vcr.slangp");
const NTSC_VR_RGBYUV: &[u8] = resource!("./ntsc_vcr/ntsc-rgbyuv.inc");
const NTSC_DECODE_FILTER_3PHASE: &[u8] = resource!("./ntsc_vcr/ntsc-decode-filter-3phase.inc");
const NTSC_PASS2_VERTEX: &[u8] = resource!("./ntsc_vcr/ntsc-pass2-vertex.inc");
const NTSC_VR_PLAY: &[u8] = resource!("./ntsc_vcr/play.png");
const NTSC_VR_STATIC: &[u8] = resource!("./ntsc_vcr/static.slang");

pub fn ntscvcr() -> Result<ShaderPreset, LoadError> {
    let dir_path = Path::new("/tmp/ntsc_vcr");
    if !dir_path.exists() {
        fs::create_dir_all(dir_path)?;
    }

    let files = vec![
        ("image-adjustment.slang", NTSC_VR_IMAGE_ADJUSTMENT),
        ("image-adjustment.slangp", NTSC_VR_IMAGE_ADJUSTMENTP),
        ("ntsc-pass1-composite-3phase.slang", NTSC_VR_NTSC_PASS1_COMPOSITE_3PHASE),
        ("ntsc-pass2-3phase.slang", NTSC_VR_NTSC_PASS2_3PHASE),
        ("ntsc-stock.slang", NTSC_VR_STOCK),
        ("ntsc-vcr.slangp", NTSC_VR_VCR),
        ("ntsc-rgbyuy.inc", NTSC_VR_RGBYUV),
        ("ntsc-decode-filter-3phase.inc", NTSC_DECODE_FILTER_3PHASE),
        ("ntsc-pass2-vertex.inc", NTSC_PASS2_VERTEX),
        ("play.png", NTSC_VR_PLAY),
        ("static.slang", NTSC_VR_STATIC),
    ];

    // Create files in the directory
    for (filename, content) in files {
        let file_path = dir_path.join(filename);
        let mut file = fs::File::create(file_path)?;
        file.write_all(content)?;
    }

    match ShaderPreset::try_parse(
        dir_path.join("ntsc-vcr.slangp"),
        ShaderFeatures::NONE,
    ) {
        Ok(preset) => Ok(preset),
        Err(err) => Err(LoadError::ParseError(err)),
    }
}

