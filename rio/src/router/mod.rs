pub mod assistant;
pub mod dialog;
pub mod settings;
pub mod welcome;

use crate::event::EventProxy;
use crate::screen::window::{configure_window, create_window_builder};
use crate::screen::Screen;
use crate::EventP;
use assistant::{Assistant, AssistantReport};
use settings::Settings;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;
use sugarloaf::font::loader;
use winit::event_loop::EventLoop;
use winit::event_loop::EventLoopWindowTarget;
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub type ErrorReport = assistant::ErrorReport;

pub struct Route {
    pub assistant: Assistant,
    pub settings: Settings,
    pub path: RoutePath,
    pub window: RouteWindow,
}

impl Route {
    #[inline]
    pub fn redraw(&self) {
        self.window.winit_window.request_redraw();
    }

    #[inline]
    pub fn open_settings(&mut self) {
        self.path = RoutePath::Settings;
    }

    #[inline]
    pub fn update_config(
        &mut self,
        config: &Rc<rio_config::Config>,
        db: &loader::Database,
    ) {
        self.window
            .screen
            .update_config(config, self.window.winit_window.theme(), db);
    }

    #[inline]
    pub fn try_close_existent_tab(&mut self) -> bool {
        self.window.screen.try_close_existent_tab()
    }

    #[inline]
    pub fn set_window_title(&mut self, title: String) {
        self.window.winit_window.set_title(&title);
    }

    #[inline]
    pub fn report_error(&mut self, error: &ErrorReport) {
        if error.report == AssistantReport::ConfigurationNotFound {
            self.path = RoutePath::Welcome;
            return;
        }

        self.assistant.set(error.to_owned());
        self.path = RoutePath::Assistant;
    }

    #[inline]
    pub fn clear_errors(&mut self) {
        self.assistant.clear();
        self.path = RoutePath::Terminal;
    }

    #[inline]
    pub fn confirm_quit(&mut self) {
        self.path = RoutePath::ConfirmQuit;
    }

    #[inline]
    pub fn quit(&mut self) {
        std::process::exit(0);
    }

    #[inline]
    pub fn has_key_wait(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        if self.path == RoutePath::Terminal {
            return false;
        }

        if self.path == RoutePath::Settings {
            match key_event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    self.settings.move_down();
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.settings.move_up();
                }
                Key::Named(NamedKey::ArrowLeft) => {
                    self.settings.move_left();
                }
                Key::Named(NamedKey::ArrowRight) => {
                    self.settings.move_right();
                }
                Key::Named(NamedKey::Enter) => {
                    self.settings.write_current_config_into_file();
                    self.path = RoutePath::Terminal;
                }
                Key::Named(NamedKey::Escape) => {
                    self.settings.reset();
                    self.path = RoutePath::Terminal;
                }
                _ => {}
            }

            return true;
        }

        let is_enter = key_event.logical_key == Key::Named(NamedKey::Enter);
        if self.path == RoutePath::Assistant && is_enter {
            if self.assistant.is_warning() {
                self.assistant.clear();
                self.path = RoutePath::Terminal;
            }

            return true;
        }

        if self.path == RoutePath::ConfirmQuit {
            if key_event.logical_key == Key::Named(NamedKey::Escape) {
                self.quit();
            } else if is_enter {
                self.path = RoutePath::Terminal;
                return true;
            }
        }

        if self.path == RoutePath::Welcome && is_enter {
            self.settings.create_file();
            self.path = RoutePath::Terminal;

            return true;
        }

        true
    }
}

#[derive(PartialEq)]
pub enum RoutePath {
    Assistant,
    Terminal,
    Settings,
    Welcome,
    ConfirmQuit,
}

pub struct Router {
    pub routes: HashMap<WindowId, Route>,
    propagated_report: Option<ErrorReport>,
    pub font_database: loader::Database,
}

impl Router {
    pub fn new() -> Self {
        let mut font_database = loader::Database::new();
        font_database.load_system_fonts();

        Router {
            routes: HashMap::new(),
            propagated_report: None,
            font_database,
        }
    }

    #[inline]
    pub fn propagate_error_to_next_route(&mut self, error: ErrorReport) {
        self.propagated_report = Some(error);
    }

    #[inline]
    pub fn create_route_from_window(&mut self, route_window: RouteWindow) {
        let id = route_window.winit_window.id();
        let mut route = Route {
            window: route_window,
            path: RoutePath::Terminal,
            settings: Settings::new(&self.font_database),
            assistant: Assistant::new(),
        };

        if let Some(err) = &self.propagated_report {
            route.report_error(err);
            self.propagated_report = None;
        }

        self.routes.insert(id, route);
    }

    #[inline]
    pub fn create_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<EventP>,
        event_proxy: EventProxy,
        config: &Rc<rio_config::Config>,
    ) {
        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_database,
            "Rio",
            None,
        );
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
                settings: Settings::new(&self.font_database),
                path: RoutePath::Terminal,
                assistant: Assistant::new(),
            },
        );
    }

    #[cfg(target_os = "macos")]
    #[inline]
    pub fn create_native_tab(
        &mut self,
        event_loop: &EventLoopWindowTarget<EventP>,
        event_proxy: EventProxy,
        config: &Rc<rio_config::Config>,
        tab_id: Option<String>,
    ) {
        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_database,
            "Rio",
            tab_id,
        );
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
                settings: Settings::new(&self.font_database),
                path: RoutePath::Terminal,
                assistant: Assistant::new(),
            },
        );
    }
}

pub struct RouteWindow {
    pub is_focused: bool,
    pub is_occluded: bool,
    pub winit_window: Window,
    pub screen: Screen,
    #[cfg(target_os = "macos")]
    pub is_macos_deadzone: bool,
}

impl RouteWindow {
    pub async fn new(
        event_loop: &EventLoop<EventP>,
        config: &Rc<rio_config::Config>,
        font_database: &loader::Database,
    ) -> Result<Self, Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let window_builder = create_window_builder("Rio", config, None);
        let winit_window = window_builder.build(event_loop).unwrap();
        let winit_window = configure_window(winit_window, config);

        let mut screen =
            Screen::new(&winit_window, config, event_proxy, font_database).await?;

        screen.init(
            screen.state.named_colors.background.1,
            config.background.mode.is_image(),
            &config.background.image,
        );

        Ok(Self {
            is_focused: false,
            is_occluded: false,
            winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        })
    }

    pub fn from_target(
        event_loop: &EventLoopWindowTarget<EventP>,
        event_proxy: EventProxy,
        config: &Rc<rio_config::Config>,
        font_database: &loader::Database,
        window_name: &str,
        tab_id: Option<String>,
    ) -> Self {
        let window_builder = create_window_builder(window_name, config, tab_id.clone());
        let winit_window = window_builder.build(event_loop).unwrap();
        let winit_window = configure_window(winit_window, config);

        let mut screen = futures::executor::block_on(Screen::new(
            &winit_window,
            config,
            event_proxy,
            font_database,
        ))
        .expect("Screen not created");

        screen.init(
            screen.state.named_colors.background.1,
            config.background.mode.is_image(),
            &config.background.image,
        );

        Self {
            is_focused: false,
            is_occluded: false,
            winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        }
    }
}
