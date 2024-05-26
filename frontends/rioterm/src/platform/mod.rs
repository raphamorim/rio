#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(use_wa)]
pub fn create_config_file() {
    use std::fs::File;
    use std::io::Write;

    let default_file_path = rio_backend::config::config_file_path();
    if default_file_path.exists() {
        return;
    }

    let default_dir_path = rio_backend::config::config_dir_path();
    match std::fs::create_dir_all(&default_dir_path) {
        Ok(_) => {
            log::info!("configuration path created {}", default_dir_path.display());
        }
        Err(err_message) => {
            log::error!("could not create config directory: {err_message}");
        }
    }

    match File::create(&default_file_path) {
        Err(err_message) => {
            log::error!(
                "could not create config file {}: {err_message}",
                default_file_path.display()
            )
        }
        Ok(mut created_file) => {
            log::info!("configuration file created {}", default_file_path.display());

            if let Err(err_message) = writeln!(
                created_file,
                "{}",
                rio_backend::config::config_file_content()
            ) {
                log::error!("could not update config file with defaults: {err_message}")
            }
        }
    }
}
