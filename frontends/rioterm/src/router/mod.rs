pub mod routes;
mod window;
use crate::event::EventProxy;
use crate::router::window::{configure_window, create_window_builder};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::{Screen, ScreenWindowProperties};
use assistant::Assistant;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_backend::clipboard::Clipboard;
use rio_backend::config::Config as RioConfig;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};
use rio_backend::event::{EventPayload, RioEvent, RioEventType};
use rio_window::event_loop::ActiveEventLoop;
use rio_window::keyboard::{Key, NamedKey};
#[cfg(not(any(target_os = "macos", windows)))]
use rio_window::platform::startup_notify::{
    self, EventLoopExtStartupNotify, WindowAttributesExtStartupNotify,
};
use rio_window::window::{Window, WindowId};
use routes::{assistant, RoutePath};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

// 𜱭𜱭 unicode is not available yet for all OS
// https://www.unicode.org/charts/PDF/Unicode-16.0/U160-1CC00.pdf
// #[cfg(any(target_os = "macos", target_os = "windows"))]
// const RIO_TITLE: &str = "𜱭𜱭";
// #[cfg(not(any(target_os = "macos", target_os = "windows")))]
const RIO_TITLE: &str = "▲";

pub struct Route<'a> {
    pub assistant: assistant::Assistant,
    pub path: RoutePath,
    pub window: RouteWindow<'a>,
}

impl Route<'_> {
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

impl Route<'_> {
    #[inline]
    pub fn request_redraw(&mut self) {
        self.window.winit_window.request_redraw();
    }

    pub fn request_frame(&mut self, scheduler: &mut Scheduler) {
        let timer_id =
            TimerId::new(Topic::RenderRoute, self.window.screen.ctx().current_route());
        let event = EventPayload::new(
            RioEventType::Rio(RioEvent::Render),
            self.window.winit_window.id(),
        );

        if let Some(limit) = self.window.wait_until() {
            self.window.start_render_timestamp();
            scheduler.schedule(event, limit, false, timer_id);
        } else {
            self.window.start_render_timestamp();
            self.request_redraw();
        }
    }

    #[inline]
    pub fn update_config(
        &mut self,
        config: &RioConfig,
        db: &rio_backend::sugarloaf::font::FontLibrary,
        should_update_font: bool,
    ) {
        self.window
            .screen
            .update_config(config, db, should_update_font);
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
    pub fn has_key_wait(&mut self, key_event: &rio_window::event::KeyEvent) -> bool {
        if self.path == RoutePath::Terminal {
            return false;
        }

        let is_enter = key_event.logical_key == Key::Named(NamedKey::Enter);
        if self.path == RoutePath::Assistant {
            if self.assistant.is_warning() && is_enter {
                self.assistant.clear();
                self.path = RoutePath::Terminal;
            } else {
                return true;
            }
        }

        if self.path == RoutePath::ConfirmQuit {
            if key_event.logical_key == Key::Named(NamedKey::Escape) {
                self.path = RoutePath::Terminal;
            } else if is_enter {
                self.quit();

                return true;
            }
        }

        if self.path == RoutePath::Welcome && is_enter {
            rio_backend::config::create_config_file(None);
            self.path = RoutePath::Terminal;
        }

        false
    }
}

pub struct Router<'a> {
    pub routes: FxHashMap<WindowId, Route<'a>>,
    propagated_report: Option<RioError>,
    pub font_library: Box<rio_backend::sugarloaf::font::FontLibrary>,
    pub config_route: Option<WindowId>,
    pub clipboard: Rc<RefCell<Clipboard>>,
}

impl Router<'_> {
    pub fn new<'b>(
        fonts: rio_backend::sugarloaf::font::SugarloafFonts,
        clipboard: Clipboard,
    ) -> Router<'b> {
        let (font_library, fonts_not_found) =
            rio_backend::sugarloaf::font::FontLibrary::new(fonts);

        let mut propagated_report = None;

        if let Some(err) = fonts_not_found {
            propagated_report = Some(RioError {
                report: RioErrorType::FontsNotFound(err.fonts_not_found),
                level: RioErrorLevel::Warning,
            });
        }

        let clipboard = Rc::new(RefCell::new(clipboard));

        Router {
            routes: FxHashMap::default(),
            propagated_report,
            config_route: None,
            font_library: Box::new(font_library),
            clipboard,
        }
    }

    #[inline]
    pub fn propagate_error_to_next_route(&mut self, error: RioError) {
        self.propagated_report = Some(error);
    }

    #[inline]
    pub fn update_titles(&mut self) {
        for route in self.routes.values_mut() {
            if route.window.is_focused {
                route.window.screen.context_manager.update_titles();
            }
        }
    }

    #[inline]
    pub fn get_focused_route(&self) -> Option<WindowId> {
        self.routes
            .iter()
            .find_map(|(key, val)| {
                if val.window.winit_window.has_focus() {
                    Some(key)
                } else {
                    None
                }
            })
            .copied()
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
            self.clipboard.clone(),
        );
        let id = window.winit_window.id();
        let route = Route::new(Assistant::new(), RoutePath::Terminal, window);
        self.routes.insert(id, route);
        self.config_route = Some(id);
    }

    pub fn open_config_split(&mut self, config: &RioConfig) {
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

        let window_id = match self.get_focused_route() {
            Some(window_id) => window_id,
            None => return,
        };

        let route = match self.routes.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        route.window.screen.split_right_with_config(new_config);
    }

    #[inline]
    pub fn create_window<'a>(
        &'a mut self,
        event_loop: &'a ActiveEventLoop,
        event_proxy: EventProxy,
        config: &'a rio_backend::config::Config,
        open_url: Option<String>,
    ) {
        let tab_id = if config.navigation.is_native() {
            Some(self.routes.len().to_string())
        } else {
            None
        };

        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_library,
            RIO_TITLE,
            tab_id.as_deref(),
            open_url,
            self.clipboard.clone(),
        );
        let id = window.winit_window.id();

        let mut route = Route {
            window,
            path: RoutePath::Terminal,
            assistant: Assistant::new(),
        };

        if let Some(err) = &self.propagated_report {
            route.report_error(err);
            self.propagated_report = None;
        }

        self.routes.insert(id, route);
    }

    #[cfg(target_os = "macos")]
    #[inline]
    pub fn create_native_tab<'a>(
        &'a mut self,
        event_loop: &'a ActiveEventLoop,
        event_proxy: EventProxy,
        config: &'a rio_backend::config::Config,
        tab_id: Option<&str>,
        open_url: Option<String>,
    ) {
        let window = RouteWindow::from_target(
            event_loop,
            event_proxy,
            config,
            &self.font_library,
            RIO_TITLE,
            tab_id,
            open_url,
            self.clipboard.clone(),
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

pub struct RouteWindow<'a> {
    pub is_focused: bool,
    pub is_occluded: bool,
    has_fps_target: bool,
    pub render_timestamp: Instant,
    pub vblank_interval: Duration,
    pub winit_window: Window,
    pub screen: Screen<'a>,
    #[cfg(target_os = "macos")]
    pub is_macos_deadzone: bool,
}

impl<'a> RouteWindow<'a> {
    pub fn configure_window(&mut self, config: &rio_backend::config::Config) {
        configure_window(&self.winit_window, config);
    }

    pub fn start_render_timestamp(&mut self) {
        self.render_timestamp = Instant::now();
    }

    pub fn wait_until(&self) -> Option<Duration> {
        let elapsed_time = Instant::now()
            .duration_since(self.render_timestamp)
            .as_millis() as u64;
        let vblank_interval = self.vblank_interval.as_millis() as u64;

        match vblank_interval >= elapsed_time {
            true => Some(Duration::from_millis(vblank_interval - elapsed_time)),
            // false => None,
            false => Some(Duration::from_millis(vblank_interval.wrapping_sub(1))),
        }
    }

    pub fn update_vblank_interval(&mut self) {
        if !self.has_fps_target {
            // Get the display vblank interval.
            let monitor_vblank_interval = 1_000_000.
                / self
                    .winit_window
                    .current_monitor()
                    .and_then(|monitor| monitor.refresh_rate_millihertz())
                    .unwrap_or(60_000) as f64;

            // Now convert it to micro seconds.
            let monitor_vblank_interval =
                Duration::from_micros((1000. * monitor_vblank_interval) as u64);

            self.vblank_interval = monitor_vblank_interval;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_target<'b>(
        event_loop: &'b ActiveEventLoop,
        event_proxy: EventProxy,
        config: &'b RioConfig,
        font_library: &rio_backend::sugarloaf::font::FontLibrary,
        window_name: &str,
        tab_id: Option<&str>,
        open_url: Option<String>,
        clipboard: Rc<RefCell<Clipboard>>,
    ) -> RouteWindow<'a> {
        #[allow(unused_mut)]
        let mut window_builder = create_window_builder(window_name, config, tab_id);

        #[cfg(not(any(target_os = "macos", windows)))]
        if let Some(token) = event_loop.read_token_from_env() {
            tracing::debug!("Activating window with token: {token:?}");
            window_builder = window_builder.with_activation_token(token);

            // Remove the token from the env.
            startup_notify::reset_activation_token_env();
        }

        let winit_window = event_loop.create_window(window_builder).unwrap();
        configure_window(&winit_window, config);

        let properties = ScreenWindowProperties {
            size: winit_window.inner_size(),
            scale: winit_window.scale_factor(),
            raw_window_handle: winit_window.window_handle().unwrap().into(),
            raw_display_handle: winit_window.display_handle().unwrap().into(),
            window_id: winit_window.id(),
        };

        let screen = Screen::new(
            properties,
            config,
            event_proxy,
            font_library,
            open_url,
            clipboard,
        )
        .expect("Screen not created");

        #[cfg(target_os = "windows")]
        {
            // On windows cloak (hide) the window initially, we later reveal it after the first draw.
            // This is a workaround to hide the "white flash" that occurs during application startup.
            use rio_window::platform::windows::WindowExtWindows;
            winit_window.set_cloaked(false);
        }

        // Get the display vblank interval.
        let monitor_vblank_interval = 1_000_000.
            / winit_window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64;

        // Now convert it to micro seconds.
        let mut monitor_vblank_interval =
            Duration::from_micros((1000. * monitor_vblank_interval) as u64);
        let mut has_fps_target = false;

        if let Some(target_fps) = config.renderer.target_fps {
            monitor_vblank_interval =
                Duration::from_millis(1000 / target_fps.clamp(1, 1000));
            has_fps_target = true;
        }

        Self {
            vblank_interval: monitor_vblank_interval,
            has_fps_target,
            render_timestamp: Instant::now(),
            is_focused: true,
            is_occluded: false,
            winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        }
    }
}
