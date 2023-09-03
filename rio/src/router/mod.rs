mod assistant;
pub mod settings;

use assistant::{Assistant, AssistantReport};
use settings::Settings;

pub type ErrorReport = AssistantReport;

#[derive(PartialEq)]
pub enum Route {
    Assistant,
    Terminal,
    // Settings(should forcefully create configuration file)
    Settings(bool),
}

pub struct Router {
    pub route: Route,
    assistant: Assistant,
    pub settings: Settings,
}

impl Router {
    pub fn new() -> Self {
        Router {
            route: Route::Terminal,
            assistant: Assistant::new(),
            settings: Settings::new(),
        }
    }

    #[inline]
    pub fn report_error(&mut self, report: AssistantReport) {
        if report == AssistantReport::ConfigurationNotFound {
            self.route = Route::Settings(true);
            self.settings.create_file();
            return;
        }

        self.assistant.set(report);
        self.route = Route::Assistant;
    }

    #[inline]
    pub fn assistant_to_string(&self) -> String {
        self.assistant.to_string()
    }

    #[inline]
    pub fn clear_errors(&mut self) {
        self.assistant.clear();
        self.route = Route::Terminal;
    }
}
