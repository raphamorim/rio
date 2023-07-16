use config::{Config, Shell};
use std::fs::File;
use std::path::Path;

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

pub fn create_settings_config(config: &Config) -> Config {
    let mut editor_config = config.clone();
    #[cfg(target_os = "macos")]
    let fallback = String::from("vim");
    #[cfg(not(target_os = "macos"))]
    let fallback = String::from("vi");

    // TODO: What happens when path doesn't exist
    // Maybe run try to create
    let editor = std::env::var("EDITOR").unwrap_or(fallback);
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
