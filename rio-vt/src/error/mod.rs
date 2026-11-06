use crate::config::ConfigError;
#[cfg(feature = "renderer")]
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
    #[cfg(feature = "renderer")]
    FontsNotFound(Vec<SugarloafFont>),

    // navigation configuration has changed
    // NavigationHasChanged,
    InitializationError(String),

    // a tmux key is bound to a command that needs tmux's own display
    TmuxBindingUnsupported(String),

    // something tmux said about a command run from a prompt
    TmuxMessage(String),

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
            #[cfg(feature = "renderer")]
            RioErrorType::FontsNotFound(fonts) => {
                let mut font_str = String::from("");
                for font in fonts.iter() {
                    font_str += format!("\n• \"{}\"", font.family).as_str();
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
            RioErrorType::TmuxBindingUnsupported(command) => {
                write!(
                    f,
                    "`{command}` needs tmux's own display.\n\nA control mode client doesn't have one."
                )
            }
            RioErrorType::TmuxMessage(message) => write!(f, "{message}"),
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
