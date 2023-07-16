use config::{Config, Shell};
use std::fs::File;
use std::path::Path;

#[inline]
pub fn try_to_create_config_file(filepath: &str) {
    let file = Path::new(&filepath);
    if file.exists() {
        return;
    }

    let display = file.display();
    if let Err(err_message) = File::create(file) {
        log::error!("couldn't create config file {display}: {err_message}");
    } else {
        log::info!("configuration file created {filepath}",)
    }
}

#[inline]
pub fn create_settings_config(config: &Config) -> Config {
    let mut editor_config = config.clone();

    #[cfg(target_os = "macos")]
    let editor_fallback = String::from("vim");
    #[cfg(not(target_os = "macos"))]
    let editor_fallback = String::from("vi");

    let editor = if editor_config.editor.is_empty() {
        std::env::var("EDITOR").unwrap_or(editor_fallback)
    } else {
        editor_config.editor.to_string()
    };
    let filepath = config::config_file_path();
    try_to_create_config_file(&filepath);

    let editor_program = Shell {
        program: editor,
        args: vec![filepath],
    };
    editor_config.shell = editor_program;
    editor_config.use_fork = false;

    editor_config
}
