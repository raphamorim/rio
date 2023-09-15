use rio_config::colors::Colors;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};
use sugarloaf::components::rect::Rect;
use sugarloaf::font::{loader::Database, FONT_ID_BUILTIN};
use sugarloaf::Sugarloaf;

pub struct SettingsState {
    current: usize,
    current_item: usize,
    config: rio_config::Config,
}

pub struct Settings {
    pub default_file_path: String,
    pub default_dir_path: String,
    pub config: rio_config::Config,
    pub items: Vec<ScreenSetting>,
    pub state: SettingsState,
    pub font_families: Vec<String>,
    last_update: Instant,
}

impl Settings {
    pub fn new(db: &Database) -> Self {
        let mut font_families_hash = HashMap::new();

        for i in db.faces() {
            if !i.families.is_empty() && i.monospaced {
                font_families_hash.insert(i.families[0].0.to_owned(), true);
            }
        }

        let mut font_families = Vec::from_iter(font_families_hash.keys().cloned());
        font_families.push(String::from("Cascadia Mono (built-in)"));

        Settings {
            default_file_path: rio_config::config_file_path(),
            default_dir_path: rio_config::config_dir_path(),
            config: rio_config::Config::default(),
            items: config_to_settings_screen(
                rio_config::Config::default(),
                font_families.to_owned(),
            ),
            state: SettingsState {
                current: 0,
                current_item: 0,
                config: rio_config::Config::default(),
            },
            last_update: Instant::now(),
            font_families,
        }
    }

    #[inline]
    pub fn move_up(&mut self) {
        if self.last_update.elapsed() > Duration::from_millis(150) {
            if self.state.current == 0 {
                self.state.current = self.items.len() - 1;
            } else {
                self.state.current -= 1;
            }
            self.state.current_item = self.items[self.state.current].current;
            self.last_update = Instant::now();
        }
    }

    #[inline]
    pub fn move_down(&mut self) {
        if self.last_update.elapsed() > Duration::from_millis(150) {
            if self.state.current >= self.items.len() - 1 {
                self.state.current = 0;
            } else {
                self.state.current += 1;
            }
            self.state.current_item = self.items[self.state.current].current;
            self.last_update = Instant::now();
        }
    }

    #[inline]
    pub fn move_right(&mut self) {
        if self.last_update.elapsed() > Duration::from_millis(100) {
            if self.state.current_item >= self.items[self.state.current].options.len() - 1
            {
                self.state.current_item = 0;
            } else {
                self.state.current_item += 1;
            }
        }
        self.items[self.state.current].current = self.state.current_item;
        self.last_update = Instant::now();
    }

    #[inline]
    pub fn move_left(&mut self) {
        if self.last_update.elapsed() > Duration::from_millis(100) {
            if self.state.current_item == 0 {
                self.state.current_item =
                    self.items[self.state.current].options.len() - 1;
            } else {
                self.state.current_item -= 1;
            }
        }
        self.items[self.state.current].current = self.state.current_item;
        self.last_update = Instant::now();
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
                    writeln!(created_file, "{}", rio_config::config_file_content())
                {
                    log::error!(
                        "could not update config file with defaults: {err_message}"
                    )
                }
            }
        }
    }
}

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    named_colors: &Colors,
    settings: &crate::router::settings::Settings,
) {
    let has_changes = settings.config != settings.state.config;
    let settings_background = vec![
        Rect {
            position: [0., 100.0],
            color: named_colors.dim_black,
            size: [sugarloaf.layout.width * 2., sugarloaf.layout.height],
        },
        Rect {
            position: [0., 96.0],
            color: named_colors.blue,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., 104.0],
            color: named_colors.yellow,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., 112.0],
            color: named_colors.red,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., 180.0],
            color: named_colors.foreground,
            size: [sugarloaf.layout.width * 2., 50.],
        },
    ];

    sugarloaf.pile_rects(settings_background);

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 30.),
        "Settings".to_string(),
        FONT_ID_BUILTIN,
        28.,
        named_colors.blue,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 60.),
        format!(
            "{} • v{}",
            settings.default_file_path,
            env!("CARGO_PKG_VERSION")
        ),
        FONT_ID_BUILTIN,
        15.,
        named_colors.blue,
        false,
    );

    let items_len = settings.items.len();
    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 130.),
        String::from(""),
        7,
        16.,
        named_colors.cursor,
        true,
    );

    let previous_item = if settings.state.current > 0 {
        settings.state.current - 1
    } else {
        items_len - 1
    };

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 150.),
        format!(
            "{} | \"{}\"",
            settings.items[previous_item].title,
            settings.items[previous_item].options[settings.items[previous_item].current],
        ),
        FONT_ID_BUILTIN,
        16.,
        named_colors.dim_white,
        true,
    );

    let active_setting = &settings.items[settings.state.current];
    sugarloaf.text(
        (60., sugarloaf.layout.margin.top_y + 190.),
        format!(
            "{} | {:?}",
            active_setting.title, active_setting.options[settings.state.current_item]
        ),
        FONT_ID_BUILTIN,
        18.,
        named_colors.background.0,
        true,
    );

    if active_setting.requires_restart {
        sugarloaf.text(
            (
                sugarloaf.layout.width / sugarloaf.layout.scale_factor - 160.,
                sugarloaf.layout.margin.top_y + 225.,
            ),
            "* restart is needed".to_string(),
            FONT_ID_BUILTIN,
            14.,
            named_colors.foreground,
            true,
        );
    }

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 40.,
            sugarloaf.layout.margin.top_y + 190.,
        ),
        "󰁔".to_string(),
        7,
        28.,
        named_colors.background.0,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 190.),
        "󰁍".to_string(),
        7,
        28.,
        named_colors.background.0,
        true,
    );

    let mut iter = if settings.state.current + 5 >= items_len {
        Vec::from_iter(settings.state.current..items_len)
    } else {
        Vec::from_iter(settings.state.current..settings.state.current + 5)
    };

    let created_iter_len = iter.len();
    // Is always expected 5 items
    if created_iter_len < 5 {
        let diff = 5 - created_iter_len;
        for i in 0..diff {
            iter.push(i);
        }
    }

    let settings_iterator = Vec::from_iter(iter);

    let mut spacing_between = 230.;
    for i in settings_iterator {
        if i == settings.state.current {
            continue;
        }

        sugarloaf.text(
            (10., sugarloaf.layout.margin.top_y + spacing_between),
            format!(
                "{} | \"{}\"",
                settings.items[i].title,
                settings.items[i].options[settings.items[i].current],
            ),
            FONT_ID_BUILTIN,
            16.,
            named_colors.dim_white,
            true,
        );

        spacing_between += 20.;
    }

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + spacing_between),
        String::from(""),
        7,
        16.,
        named_colors.cursor,
        true,
    );

    let enter_button_color = if has_changes {
        named_colors.foreground
    } else {
        named_colors.background.0
    };

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 30.,
        ),
        "󰌑".to_string(),
        7,
        26.,
        enter_button_color,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 50.,
        ),
        "save".to_string(),
        FONT_ID_BUILTIN,
        14.,
        enter_button_color,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 100.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 30.,
        ),
        "󱊷".to_string(),
        7,
        26.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 100.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 50.,
        ),
        "exit".to_string(),
        FONT_ID_BUILTIN,
        14.,
        named_colors.foreground,
        true,
    );
}

pub struct ScreenSetting {
    title: String,
    options: Vec<String>,
    current: usize,
    requires_restart: bool,
}

// Falta

#[inline]
fn config_to_settings_screen(
    config: rio_config::Config,
    font_families: Vec<String>,
) -> Vec<ScreenSetting> {
    let mut settings: Vec<ScreenSetting> = vec![];
    let default_font_family = font_families.len() - 1;

    {
        let options = vec![String::from("▇"), String::from("_"), String::from("|")];
        let current: usize = options
            .iter()
            .position(|r| r == &config.cursor.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Cursor"),
            options,
            current,
            requires_restart: false,
        });
    }

    {
        let options = vec![String::from("High"), String::from("Low")];
        let current: usize = options
            .iter()
            .position(|r| r == &config.performance.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Performance"),
            options,
            current,
            requires_restart: true,
        });
    }

    {
        let options = rio_config::navigation::modes_as_vec_string();
        let current: usize = options
            .iter()
            .position(|r| r == &config.navigation.mode.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Navigation"),
            options,
            current,
            requires_restart: true,
        });
    }

    {
        let options = vec![String::from("Enabled"), String::from("Disabled")];
        let current: usize = options
            .iter()
            .position(|r| r == &config.cursor.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Blinking Cursor"),
            options,
            current,
            requires_restart: false,
        });
    }

    {
        let options: Vec<u8> = (0..20).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        if let Some(current) = options
            .iter()
            .position(|r| r == &config.padding_x.to_string())
        {
            settings.push(ScreenSetting {
                title: String::from("Padding X"),
                options,
                current,
                requires_restart: false,
            });
        }
    }

    {
        let options: Vec<u8> = (5..40).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        let current: usize = options
            .iter()
            .position(|r| r == &config.fonts.size.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Option as alt"),
            options,
            current,
            requires_restart: false,
        });
    }

    {
        let options: Vec<u8> = (5..40).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        let current: usize = options
            .iter()
            .position(|r| r == &config.fonts.size.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("New tabs using current path"),
            options,
            current,
            requires_restart: false,
        });
    }

    #[cfg(target_os = "macos")]
    {
        let options: Vec<u8> = (5..40).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        let current: usize = options
            .iter()
            .position(|r| r == &config.fonts.size.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Hide window buttons (MacOs)"),
            options,
            current,
            requires_restart: true,
        });
    }

    {
        let options: Vec<u8> = (5..40).collect();
        let options: Vec<String> = options
            .into_iter()
            .map(|c| c.to_string())
            .collect::<Vec<String>>();
        let current: usize = options
            .iter()
            .position(|r| r == &config.fonts.size.to_string())
            .unwrap_or(0);
        settings.push(ScreenSetting {
            title: String::from("Font size"),
            options,
            current,
            requires_restart: false,
        });
    }

    {
        let current: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase() == config.fonts.regular.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.push(ScreenSetting {
            title: String::from("Font family regular"),
            options: font_families.to_owned(),
            current,
            requires_restart: false,
        });
    }

    {
        let current: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase() == config.fonts.bold.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.push(ScreenSetting {
            title: String::from("Font family bold"),
            options: font_families.to_owned(),
            current,
            requires_restart: false,
        });
    }

    {
        let current: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase() == config.fonts.italic.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.push(ScreenSetting {
            title: String::from("Font family italic"),
            options: font_families.to_owned(),
            current,
            requires_restart: false,
        });
    }

    {
        let current: usize = font_families
            .iter()
            .position(|r| {
                r.to_lowercase()
                    == config.fonts.bold_italic.family.to_string().to_lowercase()
            })
            .unwrap_or(default_font_family);
        settings.push(ScreenSetting {
            title: String::from("Font family bold-italic"),
            options: font_families.to_owned(),
            current,
            requires_restart: false,
        });
    }

    // ScreenSetting {
    // title: String::from("Regular font size"),
    // options: 10..60.to_vec(),
    // });

    settings
}
