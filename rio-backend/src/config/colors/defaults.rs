use crate::config::colors::{ColorArray, ColorBuilder, ColorComposition, Format};

// These functions are expected to panic if cannot convert the hex string

#[inline]
pub fn background() -> ColorComposition {
    let color = ColorBuilder::from_hex(String::from("#0F0D0E"), Format::SRGB0_1)
        .unwrap()
        .to_arr();
    (
        color,
        wgpu::Color {
            r: color[0] as f64,
            g: color[1] as f64,
            b: color[2] as f64,
            a: color[3] as f64,
        },
    )
}

#[inline]
pub fn cursor() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F712FF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn vi_cursor() -> ColorArray {
    ColorBuilder::from_hex(String::from("#12d0ff"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn tabs() -> ColorArray {
    ColorBuilder::from_hex(String::from("#443d40"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn tabs_foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#7d7d7d"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn bar() -> ColorArray {
    ColorBuilder::from_hex(String::from("#1b1a1a"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn tabs_active() -> ColorArray {
    ColorBuilder::from_hex(String::from("#303030"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn tabs_active_foreground() -> ColorArray {
    [1., 1., 1., 1.]
}

#[inline]
pub fn tabs_active_highlight() -> ColorArray {
    ColorBuilder::from_hex(String::from("#ffa133"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn foreground() -> ColorArray {
    [1., 1., 1., 1.]
}

#[inline]
pub fn green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#2AD947"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FF1261"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#2D9AFF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FCBA28"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#393A3D"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#17d5df"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#DD30FF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn white() -> ColorArray {
    ColorBuilder::from_hex(String::from("#E7E7E7"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn default_light_black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#6B6B6B"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#82B8C8"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#93D3C3"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#AAC474"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#C28CB8"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#C55555"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_white() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F8F8F8"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn default_light_yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FECA88"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn split() -> ColorArray {
    ColorBuilder::from_hex(String::from("#292527"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn selection_foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#44C9F0"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn selection_background() -> ColorArray {
    ColorBuilder::from_hex(String::from("#1C191A"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

#[inline]
pub fn search_match_background() -> ColorArray {
    ColorBuilder::from_hex(String::from("#44C9F0"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn search_match_foreground() -> ColorArray {
    [1., 1., 1., 1.]
}
#[inline]
pub fn search_focused_match_background() -> ColorArray {
    ColorBuilder::from_hex(String::from("#E6A003"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn search_focused_match_foreground() -> ColorArray {
    [1., 1., 1., 1.]
}
#[inline]
pub fn hint_foreground() -> ColorArray {
    // Dark text color (#181818)
    ColorBuilder::from_hex(String::from("#181818"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
#[inline]
pub fn hint_background() -> ColorArray {
    // Orange background color (#f4bf75)
    ColorBuilder::from_hex(String::from("#f4bf75"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
