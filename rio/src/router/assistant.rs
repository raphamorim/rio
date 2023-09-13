use rio_config::{colors::Colors, ConfigError};
use std::fmt;
use std::fmt::Display;
use sugarloaf::components::rect::Rect;
use sugarloaf::font::FONT_ID_BUILTIN;
use sugarloaf::{font::SugarloafFont, Sugarloaf};

#[derive(Clone, Copy)]
pub enum AssistantReportLevel {
    Warning,
    Error,
}

#[derive(Clone)]
pub struct ErrorReport {
    pub report: AssistantReport,
    pub level: AssistantReportLevel,
}

impl From<ConfigError> for ErrorReport {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::ErrLoadingConfig(message) => ErrorReport {
                report: AssistantReport::InvalidConfigurationFormat(message),
                level: AssistantReportLevel::Warning,
            },
            ConfigError::ErrLoadingTheme(message) => ErrorReport {
                report: AssistantReport::InvalidConfigurationTheme(message),
                level: AssistantReportLevel::Warning,
            },
            ConfigError::PathNotFound => ErrorReport {
                report: AssistantReport::ConfigurationNotFound,
                level: AssistantReportLevel::Warning,
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum AssistantReport {
    // font was not found
    FontsNotFound(Vec<SugarloafFont>),

    // navigation configuration has changed
    // NavigationHasChanged,
    InitializationError(String),

    // configurlation file was not found
    ConfigurationNotFound,
    // configuration file have an invalid format
    InvalidConfigurationFormat(String),
    // configuration invalid theme
    InvalidConfigurationTheme(String),

    // reports that are ignored by AssistantReport
    IgnoredReport,
}

impl std::fmt::Display for AssistantReport {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AssistantReport::FontsNotFound(fonts) => {
                let mut font_str = String::from("");
                for font in fonts.iter() {
                    font_str += format!(
                        "\n• \"{}\" using {:?} {:?}",
                        font.family, font.weight, font.style
                    )
                    .as_str();
                }

                write!(f, "Font(s) not found:\n{font_str}")
            }
            AssistantReport::ConfigurationNotFound => {
                write!(f, "Configuration file was not found")
            }
            // AssistantReport::NavigationHasChanged => {
            //     write!(f, "Navigation has changed\n\nPlease reopen Rio terminal.")
            // }
            AssistantReport::InitializationError(message) => {
                write!(f, "Error initializing Rio terminal {message}")
            }
            AssistantReport::IgnoredReport => write!(f, ""),
            AssistantReport::InvalidConfigurationFormat(message) => {
                write!(f, "Found an issue loading the configuration file:\n\n{message}\n\nRio will proceed with the default configuration\nhttps://raphamorim.io/rio/docs/#configuration-file")
            }
            AssistantReport::InvalidConfigurationTheme(message) => {
                write!(f, "Found an issue in the configured theme:\n\n{message}")
            }
        }
    }
}

impl Display for Assistant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(error) = &self.inner {
            let mut assistant_report =
                String::from("------------------------------------------------\n");

            assistant_report += &error.report.to_string();

            write!(f, "{}", assistant_report)
        } else {
            write!(f, "")
        }
    }
}

pub struct Assistant {
    pub inner: Option<ErrorReport>,
}

impl Assistant {
    pub fn new() -> Assistant {
        Assistant { inner: None }
    }

    #[inline]
    pub fn set(&mut self, report: ErrorReport) {
        self.inner = Some(report);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner = None;
    }
}

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, named_colors: &Colors, content: String) {
    let assistant_background = vec![
        // Rect {
        //     position: [30., 0.0],
        //     color: self.named_colors.background.0,
        //     size: [sugarloaf.layout.width, sugarloaf.layout.height],
        // },
        Rect {
            position: [0., 30.0],
            color: named_colors.blue,
            size: [30., sugarloaf.layout.height],
        },
        Rect {
            position: [15., sugarloaf.layout.margin.top_y + 40.],
            color: named_colors.yellow,
            size: [30., sugarloaf.layout.height],
        },
        Rect {
            position: [30., sugarloaf.layout.margin.top_y + 120.],
            color: named_colors.red,
            size: [30., sugarloaf.layout.height],
        },
    ];

    sugarloaf.pile_rects(assistant_background);

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 50.),
        "Woops! Rio got warnings".to_string(),
        FONT_ID_BUILTIN,
        28.,
        named_colors.foreground,
        true,
    );

    // sugarloaf.text(
    //     (
    //         sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
    //         sugarloaf.layout.margin.top_y + 40.,
    //     ),
    //     "".to_string(),
    //     7,
    //     30.,
    //     named_colors.foreground,
    //     true,
    // );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 80.),
        "(press enter to continue)".to_string(),
        FONT_ID_BUILTIN,
        18.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 170.),
        content,
        FONT_ID_BUILTIN,
        14.,
        named_colors.foreground,
        false,
    );
}
