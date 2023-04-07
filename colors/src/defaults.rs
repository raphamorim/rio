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

pub fn tabs_active() -> ColorArray {
    ColorBuilder::from_hex(String::from("#FC7428"), Format::SRGB0_1)
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
