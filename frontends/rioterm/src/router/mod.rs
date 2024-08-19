mod window;
use crate::frame::FrameTimer;
use rio_backend::event::RioEventType;
use crate::scheduler::TimerId;
use std::time::Duration;
use crate::scheduler::{Topic, Scheduler};
use crate::event::{EventPayload, EventProxy};
use crate::router::window::{configure_window, create_window_builder};
use crate::routes::{assistant, RoutePath};
use crate::screen::{Screen, ScreenWindowProperties};
use assistant::Assistant;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_backend::config::Config as RioConfig;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use std::collections::HashMap;
use std::error::Error;
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
    /// Create a performer.
    #[inline]
    pub fn new(
        assistant: assistant::Assistant,
        path: RoutePath,
        window: RouteWindow,
    ) -> Route {
        Route {
            assistant,
            path,
            window,
        }
    }
}

impl Route {
    #[inline]
    pub fn request_redraw(&mut self) {
        self.window.winit_window.request_redraw();
    }

    /// Request a new frame for a window
    pub fn request_frame(&mut self, scheduler: &mut Scheduler) {
        // Mark that we've used a frame.
        self.window.has_frame = false;

        // Get the display vblank interval.
        let monitor_vblank_interval = 1_000_000.
            / self
                .window
                .winit_window.current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64;

        // Now convert it to micro seconds.
        let monitor_vblank_interval =
            Duration::from_micros((1000. * monitor_vblank_interval) as u64);

        let swap_timeout = self.window.frame_timer.compute_timeout(monitor_vblank_interval);

        let window_id = self.window.winit_window.id();
        let timer_id = TimerId::new(Topic::Frame, window_id);
        let event = EventPayload::new(
            RioEventType::Frame,
            window_id,
        );

        scheduler.schedule(event, swap_timeout, false, timer_id);
    }

    #[inline]
    pub fn update_config(
        &mut self,
        config: &RioConfig,
        db: &rio_backend::sugarloaf::font::FontLibrary,
    ) {
        self.window
            .screen
            .update_config(config, self.window.winit_window.theme(), db);
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
            rio_backend::config::create_config_file(None);
            self.path = RoutePath::Terminal;
            return true;
        }

        true
    }
}

pub struct Router {
    pub routes: HashMap<WindowId, Route>,
    propagated_report: Option<RioError>,
    pub font_library: Box<rio_backend::sugarloaf::font::FontLibrary>,
    pub config_route: Option<WindowId>,
}

impl Router {
    pub fn new(fonts: rio_backend::sugarloaf::font::SugarloafFonts) -> Router {
        let (font_library, fonts_not_found) =
            rio_backend::sugarloaf::font::FontLibrary::new(fonts);

        let mut propagated_report = None;

        if let Some(err) = fonts_not_found {
            propagated_report = Some(RioError {
                report: RioErrorType::FontsNotFound(err.fonts_not_found),
                level: RioErrorLevel::Warning,
            });
        }

        Router {
            routes: HashMap::default(),
            propagated_report,
            config_route: None,
            font_library: Box::new(font_library),
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
        config: &RioConfig,
    ) {
        // In case configuration window does exists already
        if let Some(route_id) = self.config_route {
            if let Some(route) = self.routes.get(&route_id) {
                route.window.winit_window.focus_window();
                return;
            }
        }

        let current_config: RioConfig = config.clone();
        let editor = config.editor.clone();
        let mut args = editor.args;
        args.push(
            rio_backend::config::config_file_path()
                .display()
                .to_string(),
        );
        let new_config = RioConfig {
            shell: rio_backend::config::Shell {
                program: editor.program,
                args,
            },
            ..current_config
        };

        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            &new_config,
            &self.font_library,
            "Rio Settings",
            None,
            None,
        );
        let id = window.winit_window.id();
        let route = Route::new(Assistant::new(), RoutePath::Terminal, window);
        self.routes.insert(id, route);
        self.config_route = Some(id);
    }

    #[inline]
    pub fn create_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &rio_backend::config::Config,
        open_url: Option<String>,
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
        config: &rio_backend::config::Config,
        tab_id: Option<String>,
        open_url: Option<String>,
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
    pub has_frame: bool,
    pub has_updates: bool,
    pub winit_window: Window,
    pub frame_timer: FrameTimer,
    pub screen: Screen<'static>,
    #[cfg(target_os = "macos")]
    pub is_macos_deadzone: bool,
}

impl RouteWindow {
    pub fn new(
        event_loop: &EventLoop<EventPayload>,
        config: &rio_backend::config::Config,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        open_url: Option<String>,
    ) -> Result<RouteWindow, Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());

        #[allow(unused_mut)]
        let mut window_builder = create_window_builder("Rio", config, None);

        #[allow(deprecated)]
        let winit_window = event_loop.create_window(window_builder).unwrap();
        let winit_window = configure_window(winit_window, config);

        let properties = ScreenWindowProperties {
            size: winit_window.inner_size(),
            scale: winit_window.scale_factor(),
            raw_window_handle: winit_window.window_handle().unwrap().into(),
            raw_display_handle: winit_window.display_handle().unwrap().into(),
            window_id: winit_window.id(),
            theme: winit_window.theme(),
        };

        let screen =
            Screen::new(properties, config, event_proxy, font_library, open_url)?;

        Ok(Self {
            is_focused: false,
            is_occluded: false,
            has_frame: true,
            has_updates: true,
            frame_timer: FrameTimer::new(),
            winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        })
    }

    pub fn from_target(
        event_loop: &ActiveEventLoop,
        event_proxy: EventProxy,
        config: &RioConfig,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        window_name: &str,
        tab_id: Option<String>,
        open_url: Option<String>,
    ) -> RouteWindow {
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

        let properties = ScreenWindowProperties {
            size: winit_window.inner_size(),
            scale: winit_window.scale_factor(),
            raw_window_handle: winit_window.window_handle().unwrap().into(),
            raw_display_handle: winit_window.display_handle().unwrap().into(),
            window_id: winit_window.id(),
            theme: winit_window.theme(),
        };

        let screen = Screen::new(properties, config, event_proxy, font_library, open_url)
            .expect("Screen not created");

        Self {
            has_frame: true,
            has_updates: true,
            frame_timer: FrameTimer::new(),
            is_focused: false,
            is_occluded: false,
            winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        }
    }
}
