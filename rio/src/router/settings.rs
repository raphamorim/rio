use rio_config::colors::Colors;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::time::{Duration, Instant};
use sugarloaf::components::rect::Rect;
use sugarloaf::font::FONT_ID_BUILTIN;
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
    last_update: Instant,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            default_file_path: rio_config::config_file_path(),
            default_dir_path: rio_config::config_dir_path(),
            config: rio_config::Config::default(),
            items: config_to_settings_screen(rio_config::Config::default()),
            state: SettingsState {
                current: 0,
                current_item: 0,
                config: rio_config::Config::default(),
            },
            last_update: Instant::now(),
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
            self.state.current_item = 0;
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
            self.state.current_item = 0;
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
        self.items[self.state.current].current =
            self.items[self.state.current].options[self.state.current_item].to_owned();
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
        self.items[self.state.current].current =
            self.items[self.state.current].options[self.state.current_item].to_owned();
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
            settings.items[previous_item].title, settings.items[previous_item].current,
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
                settings.items[i].title, settings.items[i].current,
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
    #[allow(unused)]
    options: Vec<String>,
    current: String,
    requires_restart: bool,
}

pub struct ScreenSettingOptions {
    title: String,
    value: String,
}

#[inline]
fn config_to_settings_screen(current_config: rio_config::Config) -> Vec<ScreenSetting> {
    let settings: Vec<ScreenSetting> = vec![
        ScreenSetting {
            title: String::from("Cursor"),
            options: vec![String::from("▇"), String::from("_"), String::from("|")],
            current: String::from("▇"),
            requires_restart: false,
        },
        ScreenSetting {
            title: String::from("Cursor"),
            options: vec![String::from("▇"), String::from("_"), String::from("|")],
            current: String::from("▇"),
            requires_restart: false,
        },
        ScreenSetting {
            title: String::from("Cursor"),
            options: vec![String::from("▇"), String::from("_"), String::from("|")],
            current: String::from("▇"),
            requires_restart: false,
        },
        ScreenSetting {
            title: String::from("Cursor"),
            options: vec![String::from("▇"), String::from("_"), String::from("|")],
            current: String::from("▇"),
            requires_restart: false,
        },
        ScreenSetting {
            title: String::from("Cursor"),
            options: vec![String::from("▇"), String::from("_"), String::from("|")],
            current: String::from("▇"),
            requires_restart: false,
        },
        ScreenSetting {
            title: String::from("Cursor"),
            options: vec![String::from("▇"), String::from("_"), String::from("|")],
            current: String::from("▇"),
            requires_restart: false,
        },
    ];

    // ScreenSetting {
    // title: String::from("Regular font size"),
    // options: 10..60.to_vec(),
    // });

    settings
}
