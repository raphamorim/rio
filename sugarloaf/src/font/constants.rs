#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiamono";
pub const DEFAULT_FONT_FAMILY_VARIANT: &str = "cascadiacode";

// Fonts:
// CascadiaMonoPL-Bold.ttf
// CascadiaMonoPL-BoldItalic.ttf
// CascadiaMonoPL-ExtraLight.ttf
// CascadiaMonoPL-ExtraLightItalic.ttf
// CascadiaMonoPL-Italic.ttf
// CascadiaMonoPL-Light.ttf
// CascadiaMonoPL-LightItalic.ttf
// CascadiaMonoPL-Regular.ttf
// CascadiaMonoPL-SemiBold.ttf
// CascadiaMonoPL-SemiBoldItalic.ttf
// CascadiaMonoPL-SemiLight.ttf
// CascadiaMonoPL-SemiLightItalic.ttf

pub const FONT_CASCADIAMONO_BOLD: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Bold.ttf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-BoldItalic.ttf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-ExtraLight.ttf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-ExtraLightItalic.ttf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Italic.ttf");

pub const FONT_CASCADIAMONO_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Light.ttf");

pub const FONT_CASCADIAMONO_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-LightItalic.ttf");

pub const FONT_CASCADIAMONO_REGULAR: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-Regular.ttf");

pub const FONT_CASCADIAMONO_SEMI_BOLD: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiBold.ttf");

pub const FONT_CASCADIAMONO_SEMI_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiBoldItalic.ttf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiLight.ttf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoPL-SemiLightItalic.ttf");

pub const FONT_SYMBOLS_NERD_FONT_MONO: &[u8] =
    font!("./resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");
