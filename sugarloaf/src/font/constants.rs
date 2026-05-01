#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiacode";

pub const FONT_CASCADIAMONO_BOLD: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-Bold.otf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-BoldItalic.otf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-Italic.otf");

pub const FONT_CASCADIAMONO_NF_REGULAR: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodeNF-Regular.otf");
