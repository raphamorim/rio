use crate::router::settings::Setting;
use std::collections::HashMap;

// pub const KEY_CURSOR: usize = 0;
pub const KEY_FONT_FAMILY_REGULAR: usize = 9;

#[inline]
pub fn config_to_settings(
    config: rio_config::Config,
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
            0,
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
            1,
            Setting {
                title: String::from("Performance"),
                options,
                current_option,
                requires_restart: true,
            },
        );
    }

    {
        let options = rio_config::navigation::modes_as_vec_string();
        let current_option: usize = options
            .iter()
            .position(|r| r == &config.navigation.mode.to_string())
            .unwrap_or(0);
        settings.insert(
            2,
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
            3,
            Setting {
                title: String::from("Blinking Cursor"),
                options,
                current_option,
                requires_restart: false,
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
                4,
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
            5,
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
            6,
            Setting {
                title: String::from("New tabs using current path"),
                options,
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
            7,
            Setting {
                title: String::from("Hide window buttons (MacOs)"),
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
            8,
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
            KEY_FONT_FAMILY_REGULAR,
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
            10,
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
            11,
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
            12,
            Setting {
                title: String::from("Font family Bold-Italic"),
                options: font_families.to_owned(),
                current_option,
                requires_restart: false,
            },
        );
    }

    settings
}

#[inline]
pub fn settings_to_config(settings: &HashMap<usize, Setting>) -> rio_config::Config {
    let mut current_config = rio_config::Config::load();

    {
        // if let Some(settings_cursor) = settings.get(0) {
        //     current_config.cursor = settings_cursor.options[settings_cursor.current];
        // }

        if let Some(setting) = settings.get(&KEY_FONT_FAMILY_REGULAR) {
            current_config.fonts.regular.family =
                setting.options[setting.current_option].to_owned();
        }
    }

    current_config
}
