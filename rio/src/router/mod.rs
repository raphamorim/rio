pub mod assistant;
pub mod settings;
pub mod welcome;

use assistant::{Assistant, AssistantReport};
use settings::Settings;

pub type ErrorReport = AssistantReport;

#[derive(PartialEq)]
pub enum Route {
    Assistant,
    Terminal,
    Settings,
    Welcome,
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
            self.route = Route::Welcome;
            return;
        }

        self.assistant.set(report);
        self.route = Route::Assistant;
    }

    #[inline]
    pub fn current_route_key_wait(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        if self.route == Route::Terminal {
            return false;
        }

        if key_event.logical_key == winit::keyboard::Key::Enter {
            match self.route {
                Route::Assistant => {
                    self.assistant.clear();
                    self.route = Route::Terminal;
                }
                Route::Welcome => {
                    self.settings.create_file();
                    self.route = Route::Terminal;
                }
                _ => {}
            }
        }
        return true;
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
