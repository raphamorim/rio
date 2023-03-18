pub mod gpu;

pub const LOGO_ICON: &[u8; 102762] = include_bytes!("./images/logo.ico");

pub const FONT_FIRAMONO: &[u8; 170204] =
    include_bytes!("./fonts/FiraMono/FiraMono-Regular.ttf");

pub const FONT_NOVAMONO: &[u8; 299208] =
    include_bytes!("./fonts/NovaMono/NovaMono-Regular.ttf");

// Terminal W/H contraints
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 400;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 400;

//#151515
pub const DEFAULT_COLOR_BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.021,
    g: 0.021,
    b: 0.021,
    a: 1.0,
};

// pub const DEFAULT_COLOR_BACKGROUND: wgpu::Color = wgpu::Color {
//     r: 1.0,
//     g: 1.0,
//     b: 1.0,
//     a: 1.0,
// };

// #d35100 (todo)
// #F8A145 (todo)
// #F07900 (todo)
