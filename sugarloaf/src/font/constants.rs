#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiacode";

// Fonts:
// CascadiaCode-Bold.ttf
// CascadiaCode-BoldItalic.ttf
// CascadiaCode-ExtraLight.ttf
// CascadiaCode-ExtraLightItalic.ttf
// CascadiaCode-Italic.ttf
// CascadiaCode-Light.ttf
// CascadiaCode-LightItalic.ttf
// CascadiaCode-Regular.ttf
// CascadiaCode-SemiBold.ttf
// CascadiaCode-SemiBoldItalic.ttf
// CascadiaCode-SemiLight.ttf
// CascadiaCode-SemiLightItalic.ttf

pub const FONT_CASCADIAMONO_BOLD: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-Bold.otf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-BoldItalic.otf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-ExtraLight.otf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-ExtraLightItalic.otf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-Italic.otf");

pub const FONT_CASCADIAMONO_LIGHT: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-Light.otf");

pub const FONT_CASCADIAMONO_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-LightItalic.otf");

pub const FONT_CASCADIAMONO_REGULAR: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-Regular.otf");

pub const FONT_CASCADIAMONO_SEMI_BOLD: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-SemiBold.otf");

pub const FONT_CASCADIAMONO_SEMI_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-SemiBoldItalic.otf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-SemiLight.otf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCode-SemiLightItalic.otf");

pub const FONT_SYMBOLS_NERD_FONT_MONO: &[u8] =
    font!("./resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");

pub const FONT_TWEMOJI_EMOJI: &[u8] = font!("./resources/Twemoji/Twemoji.Mozilla.ttf");
