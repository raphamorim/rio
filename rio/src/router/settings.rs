use colors::Colors;
use std::io::Write;
use sugarloaf::components::rect::Rect;
use sugarloaf::Sugarloaf;
// use config::{Config, Shell};
use std::fs::File;
use std::path::Path;

pub struct Settings {
    pub default_file_path: String,
    pub default_dir_path: String,
    pub config: config::Config,
}

impl Settings {
    pub fn new() -> Self {
        Settings {
            default_file_path: config::config_file_path(),
            default_dir_path: config::config_dir_path(),
            config: config::Config::default(),
        }
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
                    writeln!(created_file, "{}", config::config_file_content())
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
    let settings_background = vec![Rect {
        position: [00., 140.0],
        color: named_colors.foreground,
        size: [sugarloaf.layout.width, 50.],
    }];

    sugarloaf.pile_rects(settings_background);

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 30.),
        "Settings".to_string(),
        8,
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
        8,
        15.,
        named_colors.blue,
        false,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 100.),
        format!("Font Family: \"{}\"", settings.config.fonts.regular.family),
        8,
        18.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (80., sugarloaf.layout.margin.top_y + 150.),
        format!("Performance: {:?}", settings.config.performance),
        8,
        28.,
        named_colors.background.0,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 40.,
            sugarloaf.layout.margin.top_y + 150.,
        ),
        "󰁔".to_string(),
        7,
        28.,
        named_colors.background.0,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 150.),
        "󰁍".to_string(),
        7,
        28.,
        named_colors.background.0,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 200.),
        format!("Cursor: {}", settings.config.cursor),
        8,
        18.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 240.),
        format!("Navigation Mode: {:?}", settings.config.navigation.mode),
        8,
        18.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 280.),
        format!("Font Size: {}", settings.config.fonts.size),
        8,
        18.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
            sugarloaf.layout.margin.top_y + 320.,
        ),
        "󰌑".to_string(),
        7,
        26.,
        named_colors.yellow,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
            sugarloaf.layout.margin.top_y + 340.,
        ),
        "save".to_string(),
        8,
        14.,
        named_colors.yellow,
        true,
    );

    // If no changes or forced to save
    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 90.,
            sugarloaf.layout.margin.top_y + 320.,
        ),
        "󱊷".to_string(),
        7,
        26.,
        named_colors.blue,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 90.,
            sugarloaf.layout.margin.top_y + 340.,
        ),
        "exit".to_string(),
        8,
        14.,
        named_colors.blue,
        true,
    );
}
