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

pub const FONT_SYMBOLS_NERD_FONT_MONO: &[u8] =
    font!("./resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");
