/// Default Terminal.App MacOs columns/rows

pub fn default_width() -> u16 {
    662
}

pub fn default_height() -> u16 {
    438
}

pub fn default_env_vars() -> Vec<String> {
    vec![]
}

pub fn default_padding_x() -> f32 {
    10.0
}

pub fn default_option_as_alt() -> String {
    String::from("None")
}

pub fn default_log_level() -> String {
    String::from("OFF")
}

pub fn default_font() -> String {
    String::from("CascadiaMono")
}

pub fn default_cursor() -> char {
    '▇'
}

pub fn default_theme() -> String {
    String::from("")
}

pub fn default_font_size() -> f32 {
    16.0
}
