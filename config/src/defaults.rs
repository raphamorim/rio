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

pub fn default_tab_character_active() -> char {
    '●'
}

pub fn default_tab_character_inactive() -> char {
    '■'
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
pub fn default_color_green() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}
pub fn default_color_red() -> [f32; 4] {
    colors::ColorBuilder::from_hex(String::from("#FE6956"), colors::Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn default_color_blue() -> [f32; 4] {
    colors::ColorBuilder::from_hex(String::from("#5c98cd"), colors::Format::SRGB0_1)
        .unwrap()
        .to_arr()
}
pub fn default_color_yellow() -> [f32; 4] {
    [1.0, 1.0, 1.0, 1.0]
}

pub fn default_color_cursor() -> colors::Color {
    colors::ColorBuilder::from_hex(String::from("#8E12CC"), colors::Format::SRGB0_1)
        .unwrap()
        .to_wgpu()
}
