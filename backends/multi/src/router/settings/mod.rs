mod helpers;
pub mod screen;

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};
use rio_lib::sugarloaf::font::loader::Database;

pub struct SettingsState {
    current: usize,
    current_item: usize,
}

pub struct Setting {
    title: String,
    options: Vec<String>,
    current_option: usize,
    requires_restart: bool,
}

pub struct Settings {
    pub default_file_path: String,
    pub default_dir_path: String,
    pub config: rio_lib::config::Config,
    pub inner: HashMap<usize, Setting>,
    pub state: SettingsState,
    pub font_families: Vec<String>,
    last_update_y: Instant,
    last_update_x: Instant,
}

impl Settings {
    pub fn new(db: &Database) -> Self {
        let mut font_families_hash = HashMap::new();
        // TODO: Ignore families that cannot be loaded instead of manually
        // add it to a hashmap of strings
        let mut ignored_families = HashMap::new();
        ignored_families.insert(String::from("GB18030 Bitmap"), true);

        for i in db.faces() {
            if !i.families.is_empty() && i.monospaced {
                let name = i.families[0].0.to_owned();
                if ignored_families.get(&name).is_some() {
                    continue;
                }

                font_families_hash.insert(name, true);
            }
        }

        let mut font_families = Vec::from_iter(font_families_hash.keys().cloned());
        font_families.push(String::from("Cascadia Mono (built-in)"));

        Settings {
            default_file_path: rio_lib::config::config_file_path(),
            default_dir_path: rio_lib::config::config_dir_path(),
            config: rio_lib::config::Config::default(),
            inner: helpers::config_to_settings(
                rio_lib::config::Config::load(),
                font_families.to_owned(),
            ),
            state: SettingsState {
                current: 0,
                current_item: 0,
            },
            last_update_x: Instant::now(),
            last_update_y: Instant::now(),
            font_families,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.inner = helpers::config_to_settings(
            rio_lib::config::Config::load(),
            self.font_families.to_owned(),
        );
        self.state = SettingsState {
            current: 0,
            current_item: 0,
        };
    }

    #[inline]
    pub fn move_up(&mut self) {
        if self.last_update_y.elapsed() > Duration::from_millis(150) {
            self.last_update_y = Instant::now();
            if self.state.current == 0 {
                self.state.current = self.inner.len() - 1;
            } else {
                self.state.current -= 1;
            }

            if let Some(current_setting) = self.inner.get(&self.state.current) {
                self.state.current_item = current_setting.current_option;
            }
        }
    }

    #[inline]
    pub fn move_down(&mut self) {
        if self.last_update_y.elapsed() > Duration::from_millis(150) {
            self.last_update_y = Instant::now();
            if self.state.current >= self.inner.len() - 1 {
                self.state.current = 0;
            } else {
                self.state.current += 1;
            }
            if let Some(current_setting) = self.inner.get(&self.state.current) {
                self.state.current_item = current_setting.current_option;
            }
        }
    }

    #[inline]
    pub fn move_right(&mut self) {
        if self.last_update_x.elapsed() > Duration::from_millis(200) {
            self.last_update_x = Instant::now();
            if let Some(current_setting) = self.inner.get_mut(&self.state.current) {
                if self.state.current_item >= current_setting.options.len() - 1 {
                    self.state.current_item = 0;
                } else {
                    self.state.current_item += 1;
                }

                current_setting.current_option = self.state.current_item;
            }
        }
    }

    #[inline]
    pub fn move_left(&mut self) {
        if self.last_update_x.elapsed() > Duration::from_millis(200) {
            self.last_update_x = Instant::now();
            if let Some(current_setting) = self.inner.get_mut(&self.state.current) {
                if self.state.current_item == 0 {
                    self.state.current_item = current_setting.options.len() - 1;
                } else {
                    self.state.current_item -= 1;
                }

                current_setting.current_option = self.state.current_item;
            }
        }
    }

    #[inline]
    pub fn write_current_config_into_file(&mut self) {
        let config = helpers::settings_to_config(&self.inner);
        if let Ok(config_str) = config.to_string() {
            let file = Path::new(&self.default_file_path);
            match File::create(file) {
                Err(err_message) => {
                    log::error!("could not open config file: {err_message}")
                }
                Ok(mut opened_file) => {
                    if let Err(err_message) = writeln!(opened_file, "{}", config_str) {
                        log::error!(
                            "could not update config file with defaults: {err_message}"
                        )
                    }
                }
            }
        }
        self.reset();
    }

    #[inline]
    pub fn create_file(&self) {
        let file = Path::new(&self.default_file_path);
        if file.exists() {
            return;
        }

        match std::fs::create_dir_all(&self.default_dir_path) {
            Ok(_) => {
                log::info!("configuration path created {}", self.default_dir_path);
            }
            Err(err_message) => {
                log::error!("could not create config directory: {err_message}");
            }
        }

        let display = file.display();
        match File::create(file) {
            Err(err_message) => {
                log::error!("could not create config file {display}: {err_message}")
            }
            Ok(mut created_file) => {
                log::info!("configuration file created {}", self.default_file_path);

                if let Err(err_message) =
                    writeln!(created_file, "{}", rio_lib::config::config_file_content())
                {
                    log::error!(
                        "could not update config file with defaults: {err_message}"
                    )
                }
            }
        }
    }
}
