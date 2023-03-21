/// Default Terminal.App MacOs columns/rows

pub fn default_columns() -> u16 {
    80
}

pub fn default_rows() -> u16 {
    25
}

pub fn default_width() -> u16 {
    662
}

pub fn default_height() -> u16 {
    438
}

pub fn default_font_size() -> f32 {
    16.0
}

pub fn default_color_background() -> colors::Color {
    colors::ColorBuilder::from_hex(String::from("#151515"), colors::Format::SRGB0_1)
        .unwrap()
        .to_wgpu()
}

pub fn default_color_tabs_active() -> [f32; 4] {
    colors::ColorBuilder::from_hex(String::from("#F8A145"), colors::Format::SRGB0_1)
        .unwrap()
        .to_arr()
}

pub fn default_color_foreground() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

pub fn default_color_cursor() -> colors::Color {
    colors::ColorBuilder::from_hex(String::from("#8E12CC"), colors::Format::SRGB0_1)
        .unwrap()
        .to_wgpu()
}
