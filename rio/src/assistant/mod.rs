use config::ConfigError;
use std::fmt;
use std::fmt::Display;

#[derive(Clone)]
pub enum AssistantReport {
    // font family was not found
    FontFamilyNotFound(String),
    // font weight was not found
    FontWeightNotFound(String, usize),
    // configured editor not found
    EditorNotFound(String),
    // navigation configuration has changed, please reopen the terminal
    NavigationHasChanged,
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
            AssistantReport::FontFamilyNotFound(font_family) => {
                write!(f, "Font family not found:\n\n{font_family}")
            }
            AssistantReport::FontWeightNotFound(font_family, font_weight) => {
                write!(f, "Font weight not found:\n{font_family}, \n{font_weight}")
            }
            AssistantReport::EditorNotFound(editor) => {
                write!(f, "Configured editor not found:\n\n{editor}")
            }
            AssistantReport::NavigationHasChanged => {
                write!(f, "Navigation has changed\n\nPlease reopen Rio terminal.")
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

impl From<ConfigError> for AssistantReport {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::ErrLoadingConfig(message) => {
                AssistantReport::InvalidConfigurationFormat(message)
            }
            ConfigError::ErrLoadingTheme(message) => {
                AssistantReport::InvalidConfigurationTheme(message)
            }
            ConfigError::PathNotFound => AssistantReport::IgnoredReport,
        }
    }
}

impl Display for Assistant {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(report) = &self.inner {
            let mut assistant_report =
                String::from("------------------------------------------------\n");

            assistant_report += &report.to_string();

            write!(f, "{}", assistant_report)
        } else {
            write!(f, "")
        }
    }
}

pub struct Assistant {
    pub inner: Option<AssistantReport>,
}

impl Assistant {
    pub fn new() -> Assistant {
        Assistant { inner: None }
    }

    #[inline]
    pub fn set(&mut self, report: AssistantReport) {
        self.inner = Some(report);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner = None;
    }
}
