#[allow(unused_macros)]
macro_rules! font {
    ($font:literal) => {
        include_bytes!($font) as &[u8]
    };
}

pub const DEFAULT_FONT_FAMILY: &str = "cascadiacode";

// Fonts:
// CascadiaCodePL-Bold.ttf
// CascadiaCodePL-BoldItalic.ttf
// CascadiaCodePL-ExtraLight.ttf
// CascadiaCodePL-ExtraLightItalic.ttf
// CascadiaCodePL-Italic.ttf
// CascadiaCodePL-Light.ttf
// CascadiaCodePL-LightItalic.ttf
// CascadiaCodePL-Regular.ttf
// CascadiaCodePL-SemiBold.ttf
// CascadiaCodePL-SemiBoldItalic.ttf
// CascadiaCodePL-SemiLight.ttf
// CascadiaCodePL-SemiLightItalic.ttf

pub const FONT_CASCADIAMONO_BOLD: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-Bold.otf");

pub const FONT_CASCADIAMONO_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-BoldItalic.otf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-ExtraLight.otf");

pub const FONT_CASCADIAMONO_EXTRA_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-ExtraLightItalic.otf");

pub const FONT_CASCADIAMONO_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-Italic.otf");

pub const FONT_CASCADIAMONO_LIGHT: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-Light.otf");

pub const FONT_CASCADIAMONO_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-LightItalic.otf");

pub const FONT_CASCADIAMONO_REGULAR: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-Regular.otf");

pub const FONT_CASCADIAMONO_SEMI_BOLD: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-SemiBold.otf");

pub const FONT_CASCADIAMONO_SEMI_BOLD_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-SemiBoldItalic.otf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-SemiLight.otf");

pub const FONT_CASCADIAMONO_SEMI_LIGHT_ITALIC: &[u8] =
    font!("./resources/CascadiaCode/CascadiaCodePL-SemiLightItalic.otf");

pub const FONT_SYMBOLS_NERD_FONT_MONO: &[u8] =
    font!("./resources/SymbolsNerdFontMono/SymbolsNerdFontMono-Regular.ttf");

pub const FONT_TWEMOJI_EMOJI: &[u8] = font!("./resources/Twemoji/Twemoji.Mozilla.ttf");
