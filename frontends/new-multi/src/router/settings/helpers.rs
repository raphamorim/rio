use crate::router::settings::Setting;
use rio_backend::sugarloaf::font::{
    constants::DEFAULT_FONT_FAMILY, fonts::default_font_size,
};
use std::collections::HashMap;

pub const IDX_CURSOR: usize = 0;
pub const IDX_PERFORMANCE: usize = 1;
pub const IDX_NAVIGATION: usize = 2;
pub const IDX_BLINKING_CURSOR: usize = 3;
pub const IDX_PADDING_X: usize = 4;
pub const IDX_OPTION_AS_ALT: usize = 5;
pub const IDX_USE_CURRENT_PATH: usize = 6;
pub const IDX_FONT_SIZE: usize = 7;
pub const IDX_FONT_FAMILY_REGULAR: usize = 8;
pub const IDX_FONT_FAMILY_BOLD: usize = 9;
pub const IDX_FONT_FAMILY_ITALIC: usize = 10;
pub const IDX_FONT_FAMILY_BOLD_ITALIC: usize = 11;
#[cfg(target_os = "macos")]
pub const IDX_MACOS_HIDE_BUTTONS: usize = 12;

#[inline]
pub fn config_to_settings(
    config: rio_backend::config::Config,
    font_families: Vec<String>,
) -> HashMap<usize, Setting> {
    let mut settings: HashMap<usize, Setting> = HashMap::new();
    let default_font_family = font_families.len() - 1;

    {
        let options = vec![String::from("▇"), String::from("_"), String::from("|")];
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.cursor.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_CURSOR,
            Setting {
                title: String::from("Cursor"),
                options,
                current_option,
                requires_restart: false,
            },
        );
    }

    {
        let options = vec![String::from("High"), String::from("Low")];
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.performance.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_PERFORMANCE,
            Setting {
                title: String::from("Performance"),
                options,
                current_option,
                requires_restart: true,
            },
        );
    }

    {
        let options = rio_backend::config::navigation::modes_as_vec_string();
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.navigation.mode.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_NAVIGATION,
            Setting {
                title: String::from("Navigation"),
                options,
                current_option,
                requires_restart: true,
            },
        );
    }

    {
        let options = vec![String::from("false"), String::from("true")];
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.blinking_cursor.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_BLINKING_CURSOR,
            Setting {
                title: String::from("Blinking Cursor"),
                options,
                current_option,
                requires_restart: true,
            },
        );
    }

    {
        let options: Vec<usize> = (0..20).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        if let Some(current_option) = options
            .iter()
            .position(|r| r == &config.padding_x.to_string())
        {
            settings.insert(
                IDX_PADDING_X,
                Setting {
                    title: String::from("Padding X"),
                    options,
                    current_option,
                    requires_restart: false,
                },
            );
        }
    }

    {
        let options = vec![
            String::from("false"),
            String::from("Both"),
            String::from("Left"),
            String::from("Right"),
        ];
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.option_as_alt)
            .unwrap_or(0);
        settings.insert(
            IDX_OPTION_AS_ALT,
            Setting {
                title: String::from("Option as alt"),
                options,
                current_option,
                requires_restart: false,
            },
        );
    }

    {
        let options = vec![String::from("false"), String::from("true")];
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.navigation.use_current_path.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_USE_CURRENT_PATH,
            Setting {
                title: String::from("New tabs using current path"),
                options,
                current_option,
                requires_restart: true,
            },
        );
    }

    {
        let options: Vec<usize> = (5..40).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.fonts.size.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_FONT_SIZE,
            Setting {
                title: String::from("Font size"),
                options,
                current_option,
                requires_restart: false,
            },
        );
    }

    {
        let current_option: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase() == config.fonts.regular.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.insert(
            IDX_FONT_FAMILY_REGULAR,
            Setting {
                title: String::from("Font family Regular"),
                options: font_families.to_owned(),
                current_option,
                requires_restart: false,
            },
        );
    }

    {
        let current_option: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase() == config.fonts.bold.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.insert(
            IDX_FONT_FAMILY_BOLD,
            Setting {
                title: String::from("Font family Bold"),
                options: font_families.to_owned(),
                current_option,
                requires_restart: false,
            },
        );
    }

    {
        let current_option: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase() == config.fonts.italic.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.insert(
            IDX_FONT_FAMILY_ITALIC,
            Setting {
                title: String::from("Font family Italic"),
                options: font_families.to_owned(),
                current_option,
                requires_restart: false,
            },
        );
    }

    {
        let current_option: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase()
                    == config.fonts.bold_italic.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.insert(
            IDX_FONT_FAMILY_BOLD_ITALIC,
            Setting {
                title: String::from("Font family Bold-Italic"),
                options: font_families.to_owned(),
                current_option,
                requires_restart: false,
            },
        );
    }

    #[cfg(target_os = "macos")]
    {
        let options = vec![String::from("false"), String::from("true")];
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.navigation.macos_hide_window_buttons.to_string())
            .unwrap_or(0);
        settings.insert(
            IDX_MACOS_HIDE_BUTTONS,
            Setting {
                title: String::from("Hide window buttons (MacOs)"),
                options,
                current_option,
                requires_restart: true,
            },
        );
    }

    settings
}

#[inline]
pub fn settings_to_config(
    settings: &HashMap<usize, Setting>,
) -> rio_backend::config::Config {
    let mut current_config = rio_backend::config::Config::load();

    {
        if let Some(setting) = settings.get(&IDX_CURSOR) {
            let val = setting.options[setting.current_option]
                .parse::<char>()
                .unwrap_or_else(|_| rio_backend::config::defaults::default_cursor());
            current_config.cursor = val;
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_PERFORMANCE) {
            let val = if setting.options[setting.current_option].to_lowercase() == *"low"
            {
                rio_backend::config::Performance::Low
            } else {
                rio_backend::config::Performance::High
            };
            current_config.performance = val;
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_NAVIGATION) {
            let val = setting.options[setting.current_option]
                .parse::<rio_backend::config::navigation::NavigationMode>()
                .unwrap_or(rio_backend::config::navigation::NavigationMode::default());
            current_config.navigation.mode = val;
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_BLINKING_CURSOR) {
            let val = setting.options[setting.current_option].to_lowercase() == *"true";
            current_config.blinking_cursor = val;
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_PADDING_X) {
            let val = setting.options[setting.current_option]
                .parse::<f32>()
                .unwrap_or_else(|_| rio_backend::config::defaults::default_padding_x());
            current_config.padding_x = val;
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_OPTION_AS_ALT) {
            current_config.option_as_alt =
                setting.options[setting.current_option].to_owned();
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_USE_CURRENT_PATH) {
            current_config.navigation.use_current_path =
                setting.options[setting.current_option].to_lowercase() == *"true";
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_FONT_SIZE) {
            let val = setting.options[setting.current_option]
                .parse::<f32>()
                .unwrap_or_else(|_| default_font_size());
            current_config.fonts.size = val;
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_FONT_FAMILY_REGULAR) {
            // In case is the last, then is the default font
            if setting.current_option == setting.options.len() - 1 {
                current_config.fonts.regular.family = DEFAULT_FONT_FAMILY.to_string();
            } else {
                current_config.fonts.regular.family =
                    setting.options[setting.current_option].to_owned();
            }
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_FONT_FAMILY_BOLD) {
            // In case is the last, then is the default font
            if setting.current_option == setting.options.len() - 1 {
                current_config.fonts.bold.family = DEFAULT_FONT_FAMILY.to_string();
            } else {
                current_config.fonts.bold.family =
                    setting.options[setting.current_option].to_owned();
            }
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_FONT_FAMILY_ITALIC) {
            // In case is the last, then is the default font
            if setting.current_option == setting.options.len() - 1 {
                current_config.fonts.italic.family = DEFAULT_FONT_FAMILY.to_string();
            } else {
                current_config.fonts.italic.family =
                    setting.options[setting.current_option].to_owned();
            }
        }
    }

    {
        if let Some(setting) = settings.get(&IDX_FONT_FAMILY_BOLD_ITALIC) {
            // In case is the last, then is the default font
            if setting.current_option == setting.options.len() - 1 {
                current_config.fonts.bold_italic.family = DEFAULT_FONT_FAMILY.to_string();
            } else {
                current_config.fonts.bold_italic.family =
                    setting.options[setting.current_option].to_owned();
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(setting) = settings.get(&IDX_MACOS_HIDE_BUTTONS) {
            let val = setting.options[setting.current_option].to_lowercase() == *"true";
            current_config.navigation.macos_hide_window_buttons = val;
        }
    }

    current_config
}
