pub fn default_env_vars() -> Vec<String> {
    vec![]
}

pub fn default_padding_x() -> f32 {
    10.
}

pub fn default_shell() -> crate::Shell {
    #[cfg(target_os = "macos")]
    {
        crate::Shell {
            program: String::from("/bin/zsh"),
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

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        crate::Shell {
            program: String::from(""),
            args: vec![],
        }
    }
}

pub fn default_working_directory() -> Option<String> {
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
    16.
}
