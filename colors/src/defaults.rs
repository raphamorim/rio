use crate::{ColorArray, ColorBuilder, ColorComposition, Format};

// These functions are expected to panic if cannot convert the hex string

pub fn background() -> ColorComposition {
    let color = ColorBuilder::from_hex(String::from("#151515"), Format::SRGB0_1).unwrap();
    (color.to_arr(), color.to_wgpu())
}

pub fn cursor() -> ColorArray {
    ColorBuilder::from_hex(String::from("#8E12CC"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn tabs_active() -> ColorArray {
    ColorBuilder::from_hex(String::from("#F8A145"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn foreground() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#00FF00"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#5C98CD"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#006EE6"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FFFF00"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#000000"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#00FFFF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FF00FF"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn tabs() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn white() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_black() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_blue() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_cyan() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_foreground() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_green() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_magenta() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_red() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_white() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn dim_yellow() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_black() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_blue() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_cyan() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_foreground() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_green() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_magenta() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_red() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_white() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn light_yellow() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}
