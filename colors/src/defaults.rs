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
    ColorBuilder::from_hex(String::from("#F1F1F1"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#1c191a"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#0e91b7"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#93d4e7"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#ecdc8a"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#098749"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#624a87"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#c7102a"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_white() -> ColorArray {
    ColorBuilder::from_hex(String::from("#c1c1c1"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn dim_yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#e6a003"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_black() -> ColorArray {
    ColorBuilder::from_hex(String::from("#2c2728"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_blue() -> ColorArray {
    ColorBuilder::from_hex(String::from("#44c9f0"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_cyan() -> ColorArray {
    ColorBuilder::from_hex(String::from("#7be1ff"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_foreground() -> ColorArray {
    ColorBuilder::from_hex(String::from("#f2efe2"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_green() -> ColorArray {
    ColorBuilder::from_hex(String::from("#0ed372"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_magenta() -> ColorArray {
    ColorBuilder::from_hex(String::from("#9e88be"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_red() -> ColorArray {
    ColorBuilder::from_hex(String::from("#f25e73"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn light_white() -> ColorArray {
    [1.0, 1.0, 1.0, 1.0]
}

pub fn light_yellow() -> ColorArray {
    ColorBuilder::from_hex(String::from("#fdd170"), Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
