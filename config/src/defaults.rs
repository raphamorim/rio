pub fn default_env_vars() -> Vec<String> {
    vec![]
}

pub fn default_padding_x() -> f32 {
    #[cfg(not(target_os = "macos"))]
    {
        0.
    }

    #[cfg(target_os = "macos")]
    {
        10.
    }
}

pub fn default_line_height() -> f32 {
    1.0
}

pub fn default_shell() -> crate::Shell {
    #[cfg(not(target_os = "windows"))]
    {
        crate::Shell {
            program: String::from(""),
            args: vec![String::from("--login")],
        }
    }

    #[cfg(target_os = "windows")]
    {
        crate::Shell {
            program: String::from("powershell"),
            args: vec![],
        }
    }
}

pub fn default_editor() -> String {
    String::from("")
}

pub fn default_use_fork() -> bool {
    #[cfg(target_os = "macos")]
    {
        false
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn default_working_dir() -> Option<String> {
    None
}

pub fn default_window_opacity() -> f32 {
    1.
}

pub fn default_option_as_alt() -> String {
    String::from("None")
}

pub fn default_log_level() -> String {
    String::from("OFF")
}

pub fn default_cursor() -> char {
    '▇'
}

pub fn default_theme() -> String {
    String::from("")
}

pub fn default_window_width() -> i32 {
    600
}

pub fn default_window_height() -> i32 {
    400
}
