pub mod assistant;
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
use winit::event_loop::EventLoop;
use winit::event_loop::EventLoopWindowTarget;
use winit::window::Window;
use winit::window::WindowId;

pub type ErrorReport = AssistantReport;

pub struct Route {
    assistant: Assistant,
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
    pub fn update_config(&mut self, config: &Rc<config::Config>) {
        self.window.screen.update_config(config);
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
    pub fn report_error(&mut self, report: AssistantReport) {
        if report == AssistantReport::ConfigurationNotFound {
            self.path = RoutePath::Welcome;
            return;
        }

        self.assistant.set(report);
        self.path = RoutePath::Assistant;
    }

    #[inline]
    pub fn assistant_to_string(&self) -> String {
        self.assistant.to_string()
    }

    #[inline]
    pub fn clear_errors(&mut self) {
        self.assistant.clear();
        self.path = RoutePath::Terminal;
    }

    #[inline]
    pub fn has_key_wait(&mut self, key_event: &winit::event::KeyEvent) -> bool {
        if self.path == RoutePath::Terminal {
            return false;
        }

        if key_event.logical_key == winit::keyboard::Key::Enter {
            match self.path {
                RoutePath::Assistant => {
                    self.assistant.clear();
                    self.path = RoutePath::Terminal;
                }
                RoutePath::Welcome => {
                    self.settings.create_file();
                    self.path = RoutePath::Terminal;
                }
                RoutePath::Settings => {
                    // self.settings.create_file();
                    self.path = RoutePath::Terminal;
                }
                _ => {}
            }
        }
        true
    }
}

#[derive(PartialEq)]
pub enum RoutePath {
    Assistant,
    Terminal,
    #[allow(dead_code)]
    Settings,
    Welcome,
}

pub struct Router {
    pub routes: HashMap<WindowId, Route>,
    propagated_report: Option<AssistantReport>,
}

impl Router {
    pub fn new() -> Self {
        Router {
            routes: HashMap::new(),
            propagated_report: None,
        }
    }

    pub fn propagate_error_to_next_route(&mut self, report: AssistantReport) {
        self.propagated_report = Some(report);
    }

    #[inline]
    pub fn create_route_from_window(&mut self, route_window: RouteWindow) {
        let id = route_window.winit_window.id();
        let mut route = Route {
            window: route_window,
            path: RoutePath::Terminal,
            settings: Settings::new(),
            assistant: Assistant::new(),
        };

        if let Some(err) = &self.propagated_report {
            route.report_error(err.to_owned());
            self.propagated_report = None;
        }

        self.routes.insert(id, route);
    }

    #[inline]
    pub fn create_window(
        &mut self,
        event_loop: &EventLoopWindowTarget<EventP>,
        event_proxy: EventProxy,
        config: &Rc<config::Config>,
    ) {
        let window =
            RouteWindow::from_target(event_loop, event_proxy, config, "Rio", None);
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
                settings: Settings::new(),
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
        config: &Rc<config::Config>,
        tab_id: Option<String>,
    ) {
        let window =
            RouteWindow::from_target(event_loop, event_proxy, config, "Rio", tab_id);
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
                settings: Settings::new(),
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
        config: &Rc<config::Config>,
    ) -> Result<Self, Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let window_builder = create_window_builder("Rio", config, None);
        let winit_window = window_builder.build(event_loop).unwrap();
        let winit_window = configure_window(winit_window, config);

        let mut screen = Screen::new(&winit_window, config, event_proxy, None).await?;

        screen.init(config.colors.background.1);

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
        config: &Rc<config::Config>,
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
            tab_id,
        ))
        .expect("Screen not created");

        screen.init(config.colors.background.1);

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
