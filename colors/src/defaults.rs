use crate::{ColorArray, ColorBuilder, ColorComposition, Format};

// These functions are expected to panic if cannot convert the hex string

pub fn background() -> ColorComposition {
    let color = ColorBuilder::from_hex(String::from("#0F0D0E"), Format::SRGB0_1).unwrap();
    (color.to_arr(), color.to_wgpu())
}

pub fn cursor() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F38BA3"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn tabs() -> ColorArray {
    ColorBuilder::from_hex(String::from("#12B5E5"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn tabs_active() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FCBA28"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F9F4DA"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#0BA95B"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#ED203D"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#12B5E5"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FCBA28"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#231F20"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#88DAF2"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#7B5EA7"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn white() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F1F1F1"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#1C191A"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#0E91B7"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#93D4E7"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#ECDC8A"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#098749"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#624A87"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#C7102A"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_white() -> ColorArray {
    ColorBuilder::from_hex(String::from("#C1C1C1"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#E6A003"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#ADA8A0"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#44C9F0"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#7BE1FF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F2EFE2"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#0ED372"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#9E88BE"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F25E73"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_white() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FFFFFF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FDF170"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
