mod assistant;
mod settings;

use assistant::{Assistant, AssistantReport};

pub type ErrorReport = AssistantReport;

#[derive(PartialEq)]
pub enum Route {
    Assistant,
    Settings(bool),
    Terminal,
}

pub struct Router {
    pub route: Route,
    assistant: Assistant,
}

impl Router {
    pub fn new() -> Self {
        Router {
            route: Route::Terminal,
            assistant: Assistant::new(),
        }
    }

    #[inline]
    pub fn report_error(&mut self, report: AssistantReport) {
        if report == AssistantReport::ConfigurationNotFound {
            self.route = Route::Settings(true);
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
