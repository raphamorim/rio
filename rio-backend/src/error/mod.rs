use crate::config::ConfigError;
use crate::sugarloaf::font::SugarloafFont;

#[derive(Clone, Copy, PartialEq)]
pub enum RioErrorLevel {
    Warning,
    Error,
}

#[derive(Clone)]
pub struct RioError {
    pub report: RioErrorType,
    pub level: RioErrorLevel,
}

impl RioError {
    pub fn configuration_not_found() -> Self {
        RioError {
            level: RioErrorLevel::Warning,
            report: RioErrorType::ConfigurationNotFound,
        }
    }
}

impl From<ConfigError> for RioError {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::ErrLoadingConfig(message) => RioError {
                report: RioErrorType::InvalidConfigurationFormat(message),
                level: RioErrorLevel::Warning,
            },
            ConfigError::ErrLoadingTheme(message) => RioError {
                report: RioErrorType::InvalidConfigurationTheme(message),
                level: RioErrorLevel::Warning,
            },
            ConfigError::PathNotFound => RioError {
                report: RioErrorType::ConfigurationNotFound,
                level: RioErrorLevel::Warning,
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum RioErrorType {
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

    // background image referenced in config could not be loaded
    BackgroundImageLoadFailure(String),

    // reports that are ignored by RioErrorType
    IgnoredReport,
}

impl std::fmt::Display for RioErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            RioErrorType::FontsNotFound(fonts) => {
                let mut font_str = String::from("");
                for font in fonts.iter() {
                    let style = match &font.style {
                        crate::sugarloaf::font::fonts::FontStyle::Default => {
                            String::from("default style")
                        }
                        crate::sugarloaf::font::fonts::FontStyle::Disabled => {
                            String::from("disabled")
                        }
                        crate::sugarloaf::font::fonts::FontStyle::Named(s) => {
                            format!("style \"{s}\"")
                        }
                    };
                    font_str +=
                        format!("\n• \"{}\" using {}", font.family, style).as_str();
                }

                write!(f, "Font(s) not found:\n{font_str}")
            }
            RioErrorType::ConfigurationNotFound => {
                write!(f, "Configuration file was not found")
            }
            // RioErrorType::NavigationHasChanged => {
            //     write!(f, "Navigation has changed\n\nPlease reopen Rio terminal.")
            // }
            RioErrorType::InitializationError(message) => {
                write!(f, "Error initializing Rio terminal:\n{message}")
            }
            RioErrorType::IgnoredReport => write!(f, ""),
            RioErrorType::InvalidConfigurationFormat(message) => {
                write!(f, "Found an issue loading the configuration file:\n\n{message}\n\nRio will proceed with the default configuration")
            }
            RioErrorType::InvalidConfigurationTheme(message) => {
                write!(f, "Found an issue in the configured theme:\n\n{message}")
            }
            RioErrorType::BackgroundImageLoadFailure(message) => {
                write!(
                    f,
                    "Could not load the configured background image:\n\n{message}\n\nCheck `window.background-image.path` in your config."
                )
            }
        }
    }
}
