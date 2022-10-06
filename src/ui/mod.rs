pub const LOGO_ICON: &[u8; 102762] = include_bytes!("./images/logo.ico");

pub const FONT_FIRA_MONO: &[u8; 170204] =
    include_bytes!("./fonts/Fira_Mono/FiraMono-Regular.ttf");

// Terminal W/H contraints
pub const DEFAULT_WINDOW_HEIGHT: i32 = 400;
pub const DEFAULT_WINDOW_WIDTH: i32 = 600;
pub const DEFAULT_MINIMUM_WINDOW_HEIGHT: i32 = 300;
pub const DEFAULT_MINIMUM_WINDOW_WIDTH: i32 = 400;

// #151515
pub const DEFAULT_COLOR_BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.021,
    g: 0.021,
    b: 0.021,
    a: 1.0,
};

// #d35100 (todo)
pub const DEFAULT_COLOR_LINE: wgpu::Color = wgpu::Color {
    r: 0.8274509804,
    g: 0.3176470588,
    b: 0.0,
    a: 1.0,
};

// #F8A145 (todo)
// #F07900 (todo)
