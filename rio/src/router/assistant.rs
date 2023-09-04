use colors::Colors;
use config::ConfigError;
use std::fmt;
use std::fmt::Display;
use sugarloaf::components::rect::Rect;
use sugarloaf::Sugarloaf;

#[derive(Clone, PartialEq, Eq)]
pub enum AssistantReport {
    // font family was not found
    FontFamilyNotFound(String),
    // font weight was not found
    FontWeightNotFound(String, usize),

    // navigation configuration has changed, please reopen the terminal
    NavigationHasChanged,

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
            AssistantReport::FontFamilyNotFound(font_family) => {
                write!(f, "Font family not found:\n\n{font_family}")
            }
            AssistantReport::FontWeightNotFound(font_family, font_weight) => {
                write!(f, "Font weight not found:\n{font_family}, \n{font_weight}")
            }
            AssistantReport::ConfigurationNotFound => {
                write!(f, "Configuration file was not found")
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
            ConfigError::PathNotFound => AssistantReport::ConfigurationNotFound,
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
        "Woops! Rio got errors".to_string(),
        8,
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
        8,
        18.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 170.),
        content,
        8,
        14.,
        named_colors.foreground,
        false,
    );
}
