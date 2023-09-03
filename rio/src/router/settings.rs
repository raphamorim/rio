// use config::{Config, Shell};
use std::fs::File;
use std::path::Path;

pub struct Settings {
    pub default_path: String,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            default_path: config::config_file_path(),
        }
    }

    #[inline]
    pub fn create_file(&self) {
        let file = Path::new(&self.default_path);
        if file.exists() {
            return;
        }

        let display = file.display();
        if let Err(err_message) = File::create(file) {
            log::error!("couldn't create config file {display}: {err_message}");
        } else {
            log::info!("configuration file created {}", self.default_path);
        }
    }
}
