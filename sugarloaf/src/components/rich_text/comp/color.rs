/*!
Color representation, parsing and conversions.
*/

/// 32-bit RGBA color.
#[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Default, Debug)]
#[repr(C)]
pub struct Color {
    /// Red component.
    pub r: u8,
    /// Green component.
    pub g: u8,
    /// Blue component.
    pub b: u8,
    /// Alpha component.
    pub a: u8,
}

impl Color {
    /// Creates a new color from RGBA components.
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Creates a new color from a CSS name.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "aliceblue" => ALICE_BLUE,
            "antiquewhite" => ANTIQUE_WHITE,
            "aqua" => AQUA,
            "aquamarine" => AQUAMARINE,
            "azure" => AZURE,
            "beige" => BEIGE,
            "bisque" => BISQUE,
            "black" => BLACK,
            "blanchedalmond" => BLANCHED_ALMOND,
            "blue" => BLUE,
            "blueviolet" => BLUE_VIOLET,
            "brown" => BROWN,
            "burlywood" => BURLYWOOD,
            "cadetblue" => CADET_BLUE,
            "chartreuse" => CHARTREUSE,
            "chocolate" => CHOCOLATE,
            "coral" => CORAL,
            "cornflowerblue" => CORNFLOWER_BLUE,
            "cornsilk" => CORNSILK,
            "crimson" => CRIMSON,
            "cyan" => CYAN,
            "darkblue" => DARK_BLUE,
            "darkcyan" => DARK_CYAN,
            "darkgoldenrod" => DARK_GOLDENROD,
            "darkgray" => DARK_GRAY,
            "darkgreen" => DARK_GREEN,
            "darkkhaki" => DARK_KHAKI,
            "darkmagenta" => DARK_MAGENTA,
            "darkolivegreen" => DARK_OLIVE_GREEN,
            "darkorange" => DARK_ORANGE,
            "darkorchid" => DARK_ORCHID,
            "darkred" => DARK_RED,
            "darksalmon" => DARK_SALMON,
            "darkseagreen" => DARK_SEA_GREEN,
            "darkslateblue" => DARK_SLATE_BLUE,
            "darkslategray" => DARK_SLATE_GRAY,
            "darkturquoise" => DARK_TURQUOISE,
            "darkviolet" => DARK_VIOLET,
            "deeppink" => DEEP_PINK,
            "deepskyblue" => DEEP_SKY_BLUE,
            "dimgray" => DIM_GRAY,
            "dodgerblue" => DODGER_BLUE,
            "firebrick" => FIREBRICK,
            "floralwhite" => FLORAL_WHITE,
            "forestgreen" => FOREST_GREEN,
            "fuchsia" => FUCHSIA,
            "gainsboro" => GAINSBORO,
            "ghostwhite" => GHOST_WHITE,
            "gold" => GOLD,
            "goldenrod" => GOLDENROD,
            "gray" => GRAY,
            "green" => GREEN,
            "greenyellow" => GREEN_YELLOW,
            "honeydew" => HONEYDEW,
            "hotpink" => HOT_PINK,
            "indianred" => INDIAN_RED,
            "indigo" => INDIGO,
            "ivory" => IVORY,
            "khaki" => KHAKI,
            "lavender" => LAVENDER,
            "lavenderblush" => LAVENDER_BLUSH,
            "lawngreen" => LAWN_GREEN,
            "lemonchiffon" => LEMON_CHIFFON,
            "lightblue" => LIGHT_BLUE,
            "lightcoral" => LIGHT_CORAL,
            "lightcyan" => LIGHT_CYAN,
            "lightgoldenrodyellow" => LIGHT_GOLDENROD_YELLOW,
            "lightgray" => LIGHT_GRAY,
            "lightgreen" => LIGHT_GREEN,
            "lightpink" => LIGHT_PINK,
            "lightsalmon" => LIGHT_SALMON,
            "lightseagreen" => LIGHT_SEA_GREEN,
            "lightskyblue" => LIGHT_SKY_BLUE,
            "lightslategray" => LIGHT_SLATE_GRAY,
            "lightsteelblue" => LIGHT_STEEL_BLUE,
            "lightyellow" => LIGHT_YELLOW,
            "lime" => LIME,
            "limegreen" => LIME_GREEN,
            "linen" => LINEN,
            "magenta" => MAGENTA,
            "maroon" => MAROON,
            "mediumaquamarine" => MEDIUM_AQUAMARINE,
            "mediumblue" => MEDIUM_BLUE,
            "mediumorchid" => MEDIUM_ORCHID,
            "mediumpurple" => MEDIUM_PURPLE,
            "mediumseagreen" => MEDIUM_SEA_GREEN,
            "mediumslateblue" => MEDIUM_SLATE_BLUE,
            "mediumspringgreen" => MEDIUM_SPRING_GREEN,
            "mediumturquoise" => MEDIUM_TURQUOISE,
            "mediumvioletred" => MEDIUM_VIOLET_RED,
            "midnightblue" => MIDNIGHT_BLUE,
            "mintcream" => MINT_CREAM,
            "mistyrose" => MISTY_ROSE,
            "moccasin" => MOCCASIN,
            "navajowhite" => NAVAJO_WHITE,
            "navy" => NAVY,
            "oldlace" => OLD_LACE,
            "olive" => OLIVE,
            "olivedrab" => OLIVE_DRAB,
            "orange" => ORANGE,
            "orangered" => ORANGE_RED,
            "orchid" => ORCHID,
            "palegoldenrod" => PALE_GOLDENROD,
            "palegreen" => PALE_GREEN,
            "paleturquoise" => PALE_TURQUOISE,
            "palevioletred" => PALE_VIOLET_RED,
            "papayawhip" => PAPAYA_WHIP,
            "peachpuff" => PEACH_PUFF,
            "peru" => PERU,
            "pink" => PINK,
            "plum" => PLUM,
            "powderblue" => POWDER_BLUE,
            "purple" => PURPLE,
            "rebeccapurple" => REBECCA_PURPLE,
            "red" => RED,
            "rosybrown" => ROSY_BROWN,
            "royalblue" => ROYAL_BLUE,
            "saddlebrown" => SADDLE_BROWN,
            "salmon" => SALMON,
            "sandybrown" => SANDY_BROWN,
            "seagreen" => SEA_GREEN,
            "seashell" => SEASHELL,
            "sienna" => SIENNA,
            "silver" => SILVER,
            "skyblue" => SKY_BLUE,
            "slateblue" => SLATE_BLUE,
            "slategray" => SLATE_GRAY,
            "snow" => SNOW,
            "springgreen" => SPRING_GREEN,
            "steelblue" => STEEL_BLUE,
            "tan" => TAN,
            "teal" => TEAL,
            "thistle" => THISTLE,
            "tomato" => TOMATO,
            "transparent" => TRANSPARENT,
            "turquoise" => TURQUOISE,
            "violet" => VIOLET,
            "wheat" => WHEAT,
            "white" => WHITE,
            "whitesmoke" => WHITE_SMOKE,
            "yellow" => YELLOW,
            "yellowgreen" => YELLOW_GREEN,
            _ => return None,
        })
    }

    /// Creates a new color from a CSS style color definition.
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.starts_with("#") {
            let s = &s.as_bytes()[1..];
            let mut bytes = [0u8, 0, 0, 255];
            match s.len() {
                // RGB | RGBA
                3 | 4 => {
                    for (i, b) in s.chunks(1).enumerate() {
                        let v = core::str::from_utf8(b).ok()?;
                        let v = u8::from_str_radix(v, 16).ok()?;
                        bytes[i] = v * 16 + v;
                    }
                    return Some(bytes.into());
                }
                // RRGGBB | RRGGBBAA
                6 | 8 => {
                    for (i, b) in s.chunks(2).enumerate() {
                        let v = core::str::from_utf8(b).ok()?;
                        bytes[i] = u8::from_str_radix(v, 16).ok()?;
                    }
                    return Some(bytes.into());
                }
                _ => return None,
            }
        }
        Self::from_name(s)
    }

    /// Converts the color to an array of bytes in RGBA order.
    pub fn rgba(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    /// Converts the color to an array of bytes in ARGB order.
    pub fn argb(self) -> [u8; 4] {
        [self.a, self.r, self.g, self.b]
    }

    /// Converts the color to an array of bytes in ABGR order.
    pub fn abgr(self) -> [u8; 4] {
        [self.a, self.b, self.g, self.r]
    }    

    /// Converts the color to an array of bytes in RGB order.
    pub fn rgb(self) -> [u8; 3] {
        [self.r, self.g, self.b]
    }

    /// Converts the color to an array of bytes in BGR order.
    pub fn bgr(self) -> [u8; 3] {
        [self.b, self.g, self.r]
    }    

    /// Converts a linear RGB color to sRGB color space.
    pub fn to_srgb(self) -> Self {
        Self {
            r: LINEAR_RGB_TO_SRGB_TABLE[self.r as usize],
            g: LINEAR_RGB_TO_SRGB_TABLE[self.g as usize],
            b: LINEAR_RGB_TO_SRGB_TABLE[self.b as usize],
            a: self.a,
        }
    }

    /// Converts an sRGB color to linear color space.
    pub fn to_linear(self) -> Self {
        Self {
            r: SRGB_TO_LINEAR_RGB_TABLE[self.r as usize],
            g: SRGB_TO_LINEAR_RGB_TABLE[self.g as usize],
            b: SRGB_TO_LINEAR_RGB_TABLE[self.b as usize],
            a: self.a,
        }
    }    

    /// Converts the byte color into a floating point representation.
    pub fn to_rgba_f32(&self) -> [f32; 4] {
        let s = 1. / 255.;
        [self.r as f32 * s, self.g as f32 * s, self.b as f32 * s, self.a as f32 * s]
    }    
}

impl From<Color> for [u8; 4] {
    fn from(c: Color) -> Self {
        c.rgba()
    }
}

impl From<Color> for [u8; 3] {
    fn from(c: Color) -> Self {
        c.rgb()
    }
}

impl From<[u8; 4]> for Color {
    fn from(v: [u8; 4]) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a: v[3],
        }
    }
}

impl From<[u8; 3]> for Color {
    fn from(v: [u8; 3]) -> Self {
        Self {
            r: v[0],
            g: v[1],
            b: v[2],
            a: 255,
        }
    }
}

/// Alice blue (240, 248, 255, 255)
pub const ALICE_BLUE: Color = Color::new(240, 248, 255, 255);
/// Antique white (250, 235, 215, 255)
pub const ANTIQUE_WHITE: Color = Color::new(250, 235, 215, 255);
/// Aqua (0, 255, 255, 255)
pub const AQUA: Color = Color::new(0, 255, 255, 255);
/// Aquamarine (127, 255, 212, 255)
pub const AQUAMARINE: Color = Color::new(127, 255, 212, 255);
/// Azure (240, 255, 255, 255)
pub const AZURE: Color = Color::new(240, 255, 255, 255);
/// Beige (245, 245, 220, 255)
pub const BEIGE: Color = Color::new(245, 245, 220, 255);
/// Bisque (255, 228, 196, 255)
pub const BISQUE: Color = Color::new(255, 228, 196, 255);
/// Black (0, 0, 0, 255)
pub const BLACK: Color = Color::new(0, 0, 0, 255);
/// Blanched almond (255, 235, 205, 255)
pub const BLANCHED_ALMOND: Color = Color::new(255, 235, 205, 255);
/// Blue (0, 0, 255, 255)
pub const BLUE: Color = Color::new(0, 0, 255, 255);
/// Blue violet (138, 43, 226, 255)
pub const BLUE_VIOLET: Color = Color::new(138, 43, 226, 255);
/// Brown (165, 42, 42, 255)
pub const BROWN: Color = Color::new(165, 42, 42, 255);
/// Burlywood (222, 184, 135, 255)
pub const BURLYWOOD: Color = Color::new(222, 184, 135, 255);
/// Cadet blue (95, 158, 160, 255)
pub const CADET_BLUE: Color = Color::new(95, 158, 160, 255);
/// Chartreuse (127, 255, 0, 255)
pub const CHARTREUSE: Color = Color::new(127, 255, 0, 255);
/// Chocolate (210, 105, 30, 255)
pub const CHOCOLATE: Color = Color::new(210, 105, 30, 255);
/// Coral (255, 127, 80, 255)
pub const CORAL: Color = Color::new(255, 127, 80, 255);
/// Cornflower blue (100, 149, 237, 255)
pub const CORNFLOWER_BLUE: Color = Color::new(100, 149, 237, 255);
/// Cornsilk (255, 248, 220, 255)
pub const CORNSILK: Color = Color::new(255, 248, 220, 255);
/// Crimson (220, 20, 60, 255)
pub const CRIMSON: Color = Color::new(220, 20, 60, 255);
/// Cyan (0, 255, 255, 255)
pub const CYAN: Color = Color::new(0, 255, 255, 255);
/// Dark blue (0, 0, 139, 255)
pub const DARK_BLUE: Color = Color::new(0, 0, 139, 255);
/// Dark cyan (0, 139, 139, 255)
pub const DARK_CYAN: Color = Color::new(0, 139, 139, 255);
/// Dark goldenrod (184, 134, 11, 255)
pub const DARK_GOLDENROD: Color = Color::new(184, 134, 11, 255);
/// Dark gray (169, 169, 169, 255)
pub const DARK_GRAY: Color = Color::new(169, 169, 169, 255);
/// Dark green (0, 100, 0, 255)
pub const DARK_GREEN: Color = Color::new(0, 100, 0, 255);
/// Dark khaki (189, 183, 107, 255)
pub const DARK_KHAKI: Color = Color::new(189, 183, 107, 255);
/// Dark magenta (139, 0, 139, 255)
pub const DARK_MAGENTA: Color = Color::new(139, 0, 139, 255);
/// Dark olive green (85, 107, 47, 255)
pub const DARK_OLIVE_GREEN: Color = Color::new(85, 107, 47, 255);
/// Dark orange (255, 140, 0, 255)
pub const DARK_ORANGE: Color = Color::new(255, 140, 0, 255);
/// Dark orchid (153, 50, 204, 255)
pub const DARK_ORCHID: Color = Color::new(153, 50, 204, 255);
/// Dark red (139, 0, 0, 255)
pub const DARK_RED: Color = Color::new(139, 0, 0, 255);
/// Dark salmon (233, 150, 122, 255)
pub const DARK_SALMON: Color = Color::new(233, 150, 122, 255);
/// Dark sea green (143, 188, 143, 255)
pub const DARK_SEA_GREEN: Color = Color::new(143, 188, 143, 255);
/// Dark slate blue (72, 61, 139, 255)
pub const DARK_SLATE_BLUE: Color = Color::new(72, 61, 139, 255);
/// Dark slate gray (47, 79, 79, 255)
pub const DARK_SLATE_GRAY: Color = Color::new(47, 79, 79, 255);
/// Dark turquoise (0, 206, 209, 255)
pub const DARK_TURQUOISE: Color = Color::new(0, 206, 209, 255);
/// Dark violet (148, 0, 211, 255)
pub const DARK_VIOLET: Color = Color::new(148, 0, 211, 255);
/// Deep pink (255, 20, 147, 255)
pub const DEEP_PINK: Color = Color::new(255, 20, 147, 255);
/// Deep sky blue (0, 191, 255, 255)
pub const DEEP_SKY_BLUE: Color = Color::new(0, 191, 255, 255);
/// Dim gray (105, 105, 105, 255)
pub const DIM_GRAY: Color = Color::new(105, 105, 105, 255);
/// Dodger blue (30, 144, 255, 255)
pub const DODGER_BLUE: Color = Color::new(30, 144, 255, 255);
/// Firebrick (178, 34, 34, 255)
pub const FIREBRICK: Color = Color::new(178, 34, 34, 255);
/// Floral white (255, 250, 240, 255)
pub const FLORAL_WHITE: Color = Color::new(255, 250, 240, 255);
/// Forest green (34, 139, 34, 255)
pub const FOREST_GREEN: Color = Color::new(34, 139, 34, 255);
/// Fuchsia (255, 0, 255, 255)
pub const FUCHSIA: Color = Color::new(255, 0, 255, 255);
/// Gainsboro (220, 220, 220, 255)
pub const GAINSBORO: Color = Color::new(220, 220, 220, 255);
/// Ghost white (248, 248, 255, 255)
pub const GHOST_WHITE: Color = Color::new(248, 248, 255, 255);
/// Gold (255, 215, 0, 255)
pub const GOLD: Color = Color::new(255, 215, 0, 255);
/// Goldenrod (218, 165, 32, 255)
pub const GOLDENROD: Color = Color::new(218, 165, 32, 255);
/// Gray (128, 128, 128, 255)
pub const GRAY: Color = Color::new(128, 128, 128, 255);
/// Green (0, 128, 0, 255)
pub const GREEN: Color = Color::new(0, 128, 0, 255);
/// Green yellow (173, 255, 47, 255)
pub const GREEN_YELLOW: Color = Color::new(173, 255, 47, 255);
/// Honeydew (240, 255, 240, 255)
pub const HONEYDEW: Color = Color::new(240, 255, 240, 255);
/// Hot pink (255, 105, 180, 255)
pub const HOT_PINK: Color = Color::new(255, 105, 180, 255);
/// Indian red (205, 92, 92, 255)
pub const INDIAN_RED: Color = Color::new(205, 92, 92, 255);
/// Indigo (75, 0, 130, 255)
pub const INDIGO: Color = Color::new(75, 0, 130, 255);
/// Ivory (255, 255, 240, 255)
pub const IVORY: Color = Color::new(255, 255, 240, 255);
/// Khaki (240, 230, 140, 255)
pub const KHAKI: Color = Color::new(240, 230, 140, 255);
/// Lavender (230, 230, 250, 255)
pub const LAVENDER: Color = Color::new(230, 230, 250, 255);
/// Lavender blush (255, 240, 245, 255)
pub const LAVENDER_BLUSH: Color = Color::new(255, 240, 245, 255);
/// Lawn green (124, 252, 0, 255)
pub const LAWN_GREEN: Color = Color::new(124, 252, 0, 255);
/// Lemon chiffon (255, 250, 205, 255)
pub const LEMON_CHIFFON: Color = Color::new(255, 250, 205, 255);
/// Light blue (173, 216, 230, 255)
pub const LIGHT_BLUE: Color = Color::new(173, 216, 230, 255);
/// Light coral (240, 128, 128, 255)
pub const LIGHT_CORAL: Color = Color::new(240, 128, 128, 255);
/// Light cyan (224, 255, 255, 255)
pub const LIGHT_CYAN: Color = Color::new(224, 255, 255, 255);
/// Light goldenrod yellow (250, 250, 210, 255)
pub const LIGHT_GOLDENROD_YELLOW: Color = Color::new(250, 250, 210, 255);
/// Light gray (211, 211, 211, 255)
pub const LIGHT_GRAY: Color = Color::new(211, 211, 211, 255);
/// Light green (144, 238, 144, 255)
pub const LIGHT_GREEN: Color = Color::new(144, 238, 144, 255);
/// Light pink (255, 182, 193, 255)
pub const LIGHT_PINK: Color = Color::new(255, 182, 193, 255);
/// Light salmon (255, 160, 122, 255)
pub const LIGHT_SALMON: Color = Color::new(255, 160, 122, 255);
/// Light sea green (32, 178, 170, 255)
pub const LIGHT_SEA_GREEN: Color = Color::new(32, 178, 170, 255);
/// Light sky blue (135, 206, 250, 255)
pub const LIGHT_SKY_BLUE: Color = Color::new(135, 206, 250, 255);
/// Light slate gray (119, 136, 153, 255)
pub const LIGHT_SLATE_GRAY: Color = Color::new(119, 136, 153, 255);
/// Light steel blue (176, 196, 222, 255)
pub const LIGHT_STEEL_BLUE: Color = Color::new(176, 196, 222, 255);
/// Light yellow (255, 255, 224, 255)
pub const LIGHT_YELLOW: Color = Color::new(255, 255, 224, 255);
/// Lime (0, 255, 0, 255)
pub const LIME: Color = Color::new(0, 255, 0, 255);
/// Lime green (50, 205, 50, 255)
pub const LIME_GREEN: Color = Color::new(50, 205, 50, 255);
/// Linen (250, 240, 230, 255)
pub const LINEN: Color = Color::new(250, 240, 230, 255);
/// Magenta (255, 0, 255, 255)
pub const MAGENTA: Color = Color::new(255, 0, 255, 255);
/// Maroon (128, 0, 0, 255)
pub const MAROON: Color = Color::new(128, 0, 0, 255);
/// Medium aquamarine (102, 205, 170, 255)
pub const MEDIUM_AQUAMARINE: Color = Color::new(102, 205, 170, 255);
/// Medium blue (0, 0, 205, 255)
pub const MEDIUM_BLUE: Color = Color::new(0, 0, 205, 255);
/// Medium orchid (186, 85, 211, 255)
pub const MEDIUM_ORCHID: Color = Color::new(186, 85, 211, 255);
/// Medium purple (147, 112, 219, 255)
pub const MEDIUM_PURPLE: Color = Color::new(147, 112, 219, 255);
/// Medium sea green (60, 179, 113, 255)
pub const MEDIUM_SEA_GREEN: Color = Color::new(60, 179, 113, 255);
/// Medium slate blue (123, 104, 238, 255)
pub const MEDIUM_SLATE_BLUE: Color = Color::new(123, 104, 238, 255);
/// Medium spring green (0, 250, 154, 255)
pub const MEDIUM_SPRING_GREEN: Color = Color::new(0, 250, 154, 255);
/// Medium turquoise (72, 209, 204, 255)
pub const MEDIUM_TURQUOISE: Color = Color::new(72, 209, 204, 255);
/// Medium violet red (199, 21, 133, 255)
pub const MEDIUM_VIOLET_RED: Color = Color::new(199, 21, 133, 255);
/// Midnight blue (25, 25, 112, 255)
pub const MIDNIGHT_BLUE: Color = Color::new(25, 25, 112, 255);
/// Mint cream (245, 255, 250, 255)
pub const MINT_CREAM: Color = Color::new(245, 255, 250, 255);
/// Misty rose (255, 228, 225, 255)
pub const MISTY_ROSE: Color = Color::new(255, 228, 225, 255);
/// Moccasin (255, 228, 181, 255)
pub const MOCCASIN: Color = Color::new(255, 228, 181, 255);
/// Navajo white (255, 222, 173, 255)
pub const NAVAJO_WHITE: Color = Color::new(255, 222, 173, 255);
/// Navy (0, 0, 128, 255)
pub const NAVY: Color = Color::new(0, 0, 128, 255);
/// Old lace (253, 245, 230, 255)
pub const OLD_LACE: Color = Color::new(253, 245, 230, 255);
/// Olive (128, 128, 0, 255)
pub const OLIVE: Color = Color::new(128, 128, 0, 255);
/// Olive drab (107, 142, 35, 255)
pub const OLIVE_DRAB: Color = Color::new(107, 142, 35, 255);
/// Orange (255, 165, 0, 255)
pub const ORANGE: Color = Color::new(255, 165, 0, 255);
/// Orange red (255, 69, 0, 255)
pub const ORANGE_RED: Color = Color::new(255, 69, 0, 255);
/// Orchid (218, 112, 214, 255)
pub const ORCHID: Color = Color::new(218, 112, 214, 255);
/// Pale goldenrod (238, 232, 170, 255)
pub const PALE_GOLDENROD: Color = Color::new(238, 232, 170, 255);
/// Pale green (152, 251, 152, 255)
pub const PALE_GREEN: Color = Color::new(152, 251, 152, 255);
/// Pale turquoise (175, 238, 238, 255)
pub const PALE_TURQUOISE: Color = Color::new(175, 238, 238, 255);
/// Pale violet red (219, 112, 147, 255)
pub const PALE_VIOLET_RED: Color = Color::new(219, 112, 147, 255);
/// Papaya whip (255, 239, 213, 255)
pub const PAPAYA_WHIP: Color = Color::new(255, 239, 213, 255);
/// Peach puff (255, 218, 185, 255)
pub const PEACH_PUFF: Color = Color::new(255, 218, 185, 255);
/// Peru (205, 133, 63, 255)
pub const PERU: Color = Color::new(205, 133, 63, 255);
/// Pink (255, 192, 203, 255)
pub const PINK: Color = Color::new(255, 192, 203, 255);
/// Plum (221, 160, 221, 255)
pub const PLUM: Color = Color::new(221, 160, 221, 255);
/// Powder blue (176, 224, 230, 255)
pub const POWDER_BLUE: Color = Color::new(176, 224, 230, 255);
/// Purple (128, 0, 128, 255)
pub const PURPLE: Color = Color::new(128, 0, 128, 255);
/// Rebecca purple (102, 51, 153, 255)
pub const REBECCA_PURPLE: Color = Color::new(102, 51, 153, 255);
/// Red (255, 0, 0, 255)
pub const RED: Color = Color::new(255, 0, 0, 255);
/// Rosy brown (188, 143, 143, 255)
pub const ROSY_BROWN: Color = Color::new(188, 143, 143, 255);
/// Royal blue (65, 105, 225, 255)
pub const ROYAL_BLUE: Color = Color::new(65, 105, 225, 255);
/// Saddle brown (139, 69, 19, 255)
pub const SADDLE_BROWN: Color = Color::new(139, 69, 19, 255);
/// Salmon (250, 128, 114, 255)
pub const SALMON: Color = Color::new(250, 128, 114, 255);
/// Sandy brown (244, 164, 96, 255)
pub const SANDY_BROWN: Color = Color::new(244, 164, 96, 255);
/// Sea green (46, 139, 87, 255)
pub const SEA_GREEN: Color = Color::new(46, 139, 87, 255);
/// Seashell (255, 245, 238, 255)
pub const SEASHELL: Color = Color::new(255, 245, 238, 255);
/// Sienna (160, 82, 45, 255)
pub const SIENNA: Color = Color::new(160, 82, 45, 255);
/// Silver (192, 192, 192, 255)
pub const SILVER: Color = Color::new(192, 192, 192, 255);
/// Sky blue (135, 206, 235, 255)
pub const SKY_BLUE: Color = Color::new(135, 206, 235, 255);
/// Slate blue (106, 90, 205, 255)
pub const SLATE_BLUE: Color = Color::new(106, 90, 205, 255);
/// Slate gray (112, 128, 144, 255)
pub const SLATE_GRAY: Color = Color::new(112, 128, 144, 255);
/// Snow (255, 250, 250, 255)
pub const SNOW: Color = Color::new(255, 250, 250, 255);
/// Spring green (0, 255, 127, 255)
pub const SPRING_GREEN: Color = Color::new(0, 255, 127, 255);
/// Steel blue (70, 130, 180, 255)
pub const STEEL_BLUE: Color = Color::new(70, 130, 180, 255);
/// Tan (210, 180, 140, 255)
pub const TAN: Color = Color::new(210, 180, 140, 255);
/// Teal (0, 128, 128, 255)
pub const TEAL: Color = Color::new(0, 128, 128, 255);
/// Thistle (216, 191, 216, 255)
pub const THISTLE: Color = Color::new(216, 191, 216, 255);
/// Tomato (255, 99, 71, 255)
pub const TOMATO: Color = Color::new(255, 99, 71, 255);
/// Transparent (0, 0, 0, 0)
pub const TRANSPARENT: Color = Color::new(0, 0, 0, 0);
/// Turquoise (64, 224, 208, 255)
pub const TURQUOISE: Color = Color::new(64, 224, 208, 255);
/// Violet (238, 130, 238, 255)
pub const VIOLET: Color = Color::new(238, 130, 238, 255);
/// Wheat (245, 222, 179, 255)
pub const WHEAT: Color = Color::new(245, 222, 179, 255);
/// White (255, 255, 255, 255)
pub const WHITE: Color = Color::new(255, 255, 255, 255);
/// White smoke (245, 245, 245, 255)
pub const WHITE_SMOKE: Color = Color::new(245, 245, 245, 255);
/// Yellow (255, 255, 0, 255)
pub const YELLOW: Color = Color::new(255, 255, 0, 255);
/// Yellow green (154, 205, 50, 255)
pub const YELLOW_GREEN: Color = Color::new(154, 205, 50, 255);

// "Borrowed" from the svgfilters crate:

/// Precomputed sRGB to LinearRGB table.
///
/// Since we are storing the result in `u8`, there is no need to compute those
/// values each time. Mainly because it's very expensive.
///
/// ```text
/// if (C_srgb <= 0.04045)
///     C_lin = C_srgb / 12.92;
///  else
///     C_lin = pow((C_srgb + 0.055) / 1.055, 2.4);
/// ```
///
/// Thanks to librsvg for the idea.
const SRGB_TO_LINEAR_RGB_TABLE: &[u8; 256] = &[
    0,   0,   0,   0,   0,   0,  0,    1,   1,   1,   1,   1,   1,   1,   1,   1,
    1,   1,   2,   2,   2,   2,  2,    2,   2,   2,   3,   3,   3,   3,   3,   3,
    4,   4,   4,   4,   4,   5,  5,    5,   5,   6,   6,   6,   6,   7,   7,   7,
    8,   8,   8,   8,   9,   9,  9,   10,  10,  10,  11,  11,  12,  12,  12,  13,
    13,  13,  14,  14,  15,  15,  16,  16,  17,  17,  17,  18,  18,  19,  19,  20,
    20,  21,  22,  22,  23,  23,  24,  24,  25,  25,  26,  27,  27,  28,  29,  29,
    30,  30,  31,  32,  32,  33,  34,  35,  35,  36,  37,  37,  38,  39,  40,  41,
    41,  42,  43,  44,  45,  45,  46,  47,  48,  49,  50,  51,  51,  52,  53,  54,
    55,  56,  57,  58,  59,  60,  61,  62,  63,  64,  65,  66,  67,  68,  69,  70,
    71,  72,  73,  74,  76,  77,  78,  79,  80,  81,  82,  84,  85,  86,  87,  88,
    90,  91,  92,  93,  95,  96,  97,  99, 100, 101, 103, 104, 105, 107, 108, 109,
    111, 112, 114, 115, 116, 118, 119, 121, 122, 124, 125, 127, 128, 130, 131, 133,
    134, 136, 138, 139, 141, 142, 144, 146, 147, 149, 151, 152, 154, 156, 157, 159,
    161, 163, 164, 166, 168, 170, 171, 173, 175, 177, 179, 181, 183, 184, 186, 188,
    190, 192, 194, 196, 198, 200, 202, 204, 206, 208, 210, 212, 214, 216, 218, 220,
    222, 224, 226, 229, 231, 233, 235, 237, 239, 242, 244, 246, 248, 250, 253, 255,
];

/// Precomputed LinearRGB to sRGB table.
///
/// Since we are storing the result in `u8`, there is no need to compute those
/// values each time. Mainly because it's very expensive.
///
/// ```text
/// if (C_lin <= 0.0031308)
///     C_srgb = C_lin * 12.92;
/// else
///     C_srgb = 1.055 * pow(C_lin, 1.0 / 2.4) - 0.055;
/// ```
///
/// Thanks to librsvg for the idea.
const LINEAR_RGB_TO_SRGB_TABLE: &[u8; 256] = &[
    0,  13,  22,  28,  34,  38,  42,  46,  50,  53,  56,  59,  61,  64,  66,  69,
    71,  73,  75,  77,  79,  81,  83,  85,  86,  88,  90,  92,  93,  95,  96,  98,
    99, 101, 102, 104, 105, 106, 108, 109, 110, 112, 113, 114, 115, 117, 118, 119,
    120, 121, 122, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136,
    137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148, 148, 149, 150, 151,
    152, 153, 154, 155, 155, 156, 157, 158, 159, 159, 160, 161, 162, 163, 163, 164,
    165, 166, 167, 167, 168, 169, 170, 170, 171, 172, 173, 173, 174, 175, 175, 176,
    177, 178, 178, 179, 180, 180, 181, 182, 182, 183, 184, 185, 185, 186, 187, 187,
    188, 189, 189, 190, 190, 191, 192, 192, 193, 194, 194, 195, 196, 196, 197, 197,
    198, 199, 199, 200, 200, 201, 202, 202, 203, 203, 204, 205, 205, 206, 206, 207,
    208, 208, 209, 209, 210, 210, 211, 212, 212, 213, 213, 214, 214, 215, 215, 216,
    216, 217, 218, 218, 219, 219, 220, 220, 221, 221, 222, 222, 223, 223, 224, 224,
    225, 226, 226, 227, 227, 228, 228, 229, 229, 230, 230, 231, 231, 232, 232, 233,
    233, 234, 234, 235, 235, 236, 236, 237, 237, 238, 238, 238, 239, 239, 240, 240,
    241, 241, 242, 242, 243, 243, 244, 244, 245, 245, 246, 246, 246, 247, 247, 248,
    248, 249, 249, 250, 250, 251, 251, 251, 252, 252, 253, 253, 254, 254, 255, 255,
];
