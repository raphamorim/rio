mod window;

use crate::event::{EventPayload, EventProxy};
use crate::router::window::{configure_window, create_window_builder};
use crate::routes::{assistant, RoutePath};
use crate::screen::Screen;
use assistant::Assistant;
use rio_backend::config::Config as RioConfig;
use rio_backend::error::{RioError, RioErrorType};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::rc::Rc;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, NamedKey};
#[cfg(not(any(target_os = "macos", windows)))]
use winit::platform::startup_notify::{
    self, EventLoopExtStartupNotify, WindowAttributesExtStartupNotify,
};
use winit::window::{Window, WindowId};

pub struct Route {
    pub assistant: assistant::Assistant,
    pub path: RoutePath,
    pub window: RouteWindow,
}

impl Route {
    #[inline]
    pub fn redraw(&self) {
        self.window.winit_window.request_redraw();
    }

    #[inline]
    pub fn update_config(
        &mut self,
        config: &Rc<RioConfig>,
        db: &rio_backend::sugarloaf::font::FontLibrary,
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
    #[allow(unused_variables)]
    pub fn set_window_subtitle(&mut self, subtitle: &str) {
        #[cfg(target_os = "macos")]
        self.window.winit_window.set_subtitle(subtitle);
    }

    #[inline]
    pub fn set_window_title(&mut self, title: &str) {
        self.window.winit_window.set_title(title);
    }

    #[inline]
    pub fn report_error(&mut self, error: &RioError) {
        if error.report == RioErrorType::ConfigurationNotFound {
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
                self.path = RoutePath::Terminal;
            } else if is_enter {
                self.quit();
            }

            return true;
        }

        if self.path == RoutePath::Welcome && is_enter {
            self.create_config_file();
            self.path = RoutePath::Terminal;
            return true;
        }

        true
    }

    #[inline]
    pub fn create_config_file(&self) {
        let default_file_path = rio_backend::config::config_file_path();
        if default_file_path.exists() {
            return;
        }

        let default_dir_path = rio_backend::config::config_dir_path();
        match std::fs::create_dir_all(&default_dir_path) {
            Ok(_) => {
                log::info!("configuration path created {}", default_dir_path.display());
            }
            Err(err_message) => {
                log::error!("could not create config directory: {err_message}");
            }
        }

        match File::create(&default_file_path) {
            Err(err_message) => {
                log::error!(
                    "could not create config file {}: {err_message}",
                    default_file_path.display()
                )
            }
            Ok(mut created_file) => {
                log::info!("configuration file created {}", default_file_path.display());

                if let Err(err_message) = writeln!(
                    created_file,
                    "{}",
                    rio_backend::config::config_file_content()
                ) {
                    log::error!(
                        "could not update config file with defaults: {err_message}"
                    )
                }
            }
        }
    }
}

pub struct Router {
    pub routes: HashMap<WindowId, Route>,
    propagated_report: Option<RioError>,
    pub font_library: rio_backend::sugarloaf::font::FontLibrary,
    pub config_route: Option<WindowId>,
}

impl Router {
    pub fn new() -> Self {
        let font_library = rio_backend::sugarloaf::font::FontLibrary::default();

        Router {
            routes: HashMap::new(),
            propagated_report: None,
            config_route: None,
            font_library,
        }
    }

    #[inline]
    pub fn propagate_error_to_next_route(&mut self, error: RioError) {
        self.propagated_report = Some(error);
    }

    #[inline]
    pub fn create_route_from_window(&mut self, route_window: RouteWindow) {
        let id = route_window.winit_window.id();
        let mut route = Route {
            window: route_window,
            path: RoutePath::Terminal,
            assistant: Assistant::new(),
        };

        if let Some(err) = &self.propagated_report {
            route.report_error(err);
            self.propagated_report = None;
        }

        self.routes.insert(id, route);
    }

    pub fn open_config_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &Rc<RioConfig>,
    ) {
        // In case configuration window does exists already
        if let Some(route_id) = self.config_route {
            if let Some(route) = self.routes.get(&route_id) {
                route.window.winit_window.focus_window();
                return;
            }
        }

        let current_config: RioConfig = config.as_ref().clone();
        let new_config = RioConfig {
            shell: rio_backend::config::Shell {
                program: config.editor.clone(),
                args: vec![rio_backend::config::config_file_path()
                    .display()
                    .to_string()],
            },
            ..current_config
        };

        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            &new_config.into(),
            &self.font_library,
            "Rio Settings",
            None,
            None,
        );
        let id = window.winit_window.id();
        self.routes.insert(
            id,
            Route {
                window,
                path: RoutePath::Terminal,
                assistant: Assistant::new(),
            },
        );
        self.config_route = Some(id);
    }

    #[inline]
    pub fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &Rc<RioConfig>,
        open_url: Option<&str>,
    ) {
        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_library,
            "Rio",
            None,
            open_url,
        );
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
                path: RoutePath::Terminal,
                assistant: Assistant::new(),
            },
        );
    }

    #[cfg(target_os = "macos")]
    #[inline]
    pub fn create_native_tab(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &Rc<RioConfig>,
        tab_id: Option<String>,
        open_url: Option<&str>,
    ) {
        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_library,
            "Rio",
            tab_id,
            open_url,
        );
        self.routes.insert(
            window.winit_window.id(),
            Route {
                window,
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
        event_loop: &EventLoop<EventPayload>,
        config: &Rc<RioConfig>,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        open_url: Option<&str>,
    ) -> Result<Self, Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());

        #[allow(unused_mut)]
        let mut window_builder = create_window_builder("Rio", config, None);

        #[allow(deprecated)]
        let winit_window = event_loop.create_window(window_builder).unwrap();
        let winit_window = configure_window(winit_window, config);

        let screen =
            Screen::new(&winit_window, config, event_proxy, font_library, open_url)
                .await?;

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
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &Rc<RioConfig>,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        window_name: &str,
        tab_id: Option<String>,
        open_url: Option<&str>,
    ) -> Self {
        #[allow(unused_mut)]
        let mut window_builder =
            create_window_builder(window_name, config, tab_id.clone());

        #[cfg(not(any(target_os = "macos", windows)))]
        if let Some(token) = event_loop.read_token_from_env() {
            log::debug!("Activating window with token: {token:?}");
            window_builder = window_builder.with_activation_token(token);

            // Remove the token from the env.
            startup_notify::reset_activation_token_env();
        }

        #[allow(deprecated)]
        let winit_window = event_loop.create_window(window_builder).unwrap();
        let winit_window = configure_window(winit_window, config);

        let screen = futures::executor::block_on(Screen::new(
            &winit_window,
            config,
            event_proxy,
            font_library,
            open_url,
        ))
        .expect("Screen not created");

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
