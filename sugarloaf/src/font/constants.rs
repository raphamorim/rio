#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiamono";

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
    font!("./resources/CascadiaMono/CascadiaMonoNF-Bold.ttf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-BoldItalic.ttf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-ExtraLight.ttf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-ExtraLightItalic.ttf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-Italic.ttf");

pub const FONT_CASCADIAMONO_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-Light.ttf");

pub const FONT_CASCADIAMONO_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-LightItalic.ttf");

pub const FONT_CASCADIAMONO_REGULAR: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-Regular.ttf");

pub const FONT_CASCADIAMONO_SEMI_BOLD: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-SemiBold.ttf");

pub const FONT_CASCADIAMONO_SEMI_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-SemiBoldItalic.ttf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-SemiLight.ttf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaMono/CascadiaMonoNF-SemiLightItalic.ttf");

pub const FONT_SYMBOLS_NERD_FONT_MONO: &[u8] =
    font!("./resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");
