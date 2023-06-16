pub const DEFAULT_FONT_NAME: &str = "cascadiamono";

pub const FONT_CASCADIAMONO_REGULAR: &[u8; 308212] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-Regular.otf");

pub const FONT_CASCADIAMONO_BOLD: &[u8; 312976] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-Bold.otf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8; 191296] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-Italic.otf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8; 193360] =
    include_bytes!("./resources/CascadiaMono/CascadiaMonoPL-BoldItalic.otf");

pub const FONT_EMOJI: &[u8; 877988] =
    include_bytes!("./resources/NotoEmoji/static/NotoEmoji-Regular.ttf");

#[cfg(not(target_os = "macos"))]
pub const FONT_DEJAVU_MONO: &[u8; 340712] =
    include_bytes!("./resources/DejaVuSansMono.ttf");
