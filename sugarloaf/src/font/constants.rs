#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiamono";
pub const DEFAULT_FONT_FAMILY_VARIANT: &str = "cascadiacode";

// Fonts:
// CascadiaMonoPL-Bold.otf
// CascadiaMonoPL-BoldItalic.otf
// CascadiaMonoPL-ExtraLight.otf
// CascadiaMonoPL-ExtraLightItalic.otf
// CascadiaMonoPL-Italic.otf
// CascadiaMonoPL-Light.otf
// CascadiaMonoPL-LightItalic.otf
// CascadiaMonoPL-Regular.otf
// CascadiaMonoPL-SemiBold.otf
// CascadiaMonoPL-SemiBoldItalic.otf
// CascadiaMonoPL-SemiLight.otf
// CascadiaMonoPL-SemiLightItalic.otf

pub const FONT_CASCADIAMONO_BOLD: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Bold.otf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-BoldItalic.otf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-ExtraLight.otf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-ExtraLightItalic.otf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Italic.otf");

pub const FONT_CASCADIAMONO_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Light.otf");

pub const FONT_CASCADIAMONO_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-LightItalic.otf");

pub const FONT_CASCADIAMONO_REGULAR: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Regular.otf");

pub const FONT_CASCADIAMONO_SEMI_BOLD: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiBold.otf");

pub const FONT_CASCADIAMONO_SEMI_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiBoldItalic.otf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiLight.otf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiLightItalic.otf");

// Extra

pub const FONT_EMOJI: &[u8] = font!("./resources/NotoEmoji/static/NotoEmoji-Regular.ttf");

pub const FONT_SYMBOLS_NERD_FONT_MONO: &[u8] =
    font!("./resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");

// Not macos

#[cfg(not(target_os = "macos"))]
pub const FONT_DEJAVU_SANS: &[u8] = font!("./resources/DejaVuSans/DejaVuSans.ttf");

#[cfg(not(target_os = "macos"))]
pub const FONT_UNICODE_FALLBACK: &[u8] =
    font!("./resources/chrysanthi-unicode-font/ChrysanthiUnicodeRegular-KEzo.ttf");
