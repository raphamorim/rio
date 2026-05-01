#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiacode";

/// Cascadia Code Nerd Font, upright, variable `wght` axis (200–700).
/// Used for Regular and Bold slots — same outlines, different `wght` value.
pub const FONT_CASCADIA_CODE_NF: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodeNF.ttf");

/// Cascadia Code Nerd Font, italic, variable `wght` axis (200–700).
/// Used for Italic and Bold-Italic slots — same outlines, different `wght` value.
pub const FONT_CASCADIA_CODE_NF_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodeNFItalic.ttf");

/// Default `wght` axis value used when a slot wants regular weight.
pub const WGHT_REGULAR: f32 = 400.0;

/// `wght` axis value used when a slot wants bold weight (matches the
/// Cascadia Code variable font's bold instance).
pub const WGHT_BOLD: f32 = 700.0;
