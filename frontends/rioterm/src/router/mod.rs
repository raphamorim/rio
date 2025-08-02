pub mod routes;
mod window;
use crate::event::EventProxy;
use crate::router::window::{configure_window, create_window_builder};
use crate::screen::{Screen, ScreenWindowProperties};
use assistant::Assistant;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_backend::clipboard::Clipboard;
use rio_backend::config::Config as RioConfig;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};

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

    #[inline]
    pub fn schedule_redraw(
        &mut self,
        scheduler: &mut crate::scheduler::Scheduler,
        route_id: usize,
    ) {
        #[cfg(target_os = "macos")]
        {
            // On macOS, use direct redraw as CVDisplayLink handles VSync
            let _ = (scheduler, route_id); // Suppress warnings
            self.request_redraw();
        }

        #[cfg(not(target_os = "macos"))]
        {
            use crate::event::{EventPayload, RioEvent, RioEventType};
            use crate::scheduler::{TimerId, Topic};

            // Windows and Linux use the frame scheduler with refresh rate timing
            let timer_id = TimerId::new(Topic::Render, route_id);
            let event = EventPayload::new(
                RioEventType::Rio(RioEvent::Render),
                self.window.winit_window.id(),
            );

            // Schedule a render if not already scheduled
            // Use vblank_interval for proper frame timing
            if !scheduler.scheduled(timer_id) {
                scheduler.schedule(event, self.window.vblank_interval, false, timer_id);
            }
        }
    }

    #[inline]
    pub fn begin_render(&mut self) {
        self.window.render_timestamp = Instant::now();

        // // Track frame count for performance monitoring
        // use std::collections::HashMap;
        // use std::sync::Mutex;

        // static FRAME_COUNTERS: Mutex<
        //     Option<HashMap<rio_window::window::WindowId, (u64, std::time::Instant)>>,
        // > = Mutex::new(None);
        // static LAST_LOG: Mutex<Option<std::time::Instant>> = Mutex::new(None);

        // let window_id = self.window.winit_window.id();

        // {
        //     // Use try_lock to avoid blocking other windows during performance logging
        //     let mut counters = match FRAME_COUNTERS.try_lock() {
        //         Ok(guard) => guard,
        //         Err(_) => return, // Skip performance logging if another window is using it
        //     };
        //     if counters.is_none() {
        //         *counters = Some(HashMap::new());
        //     }

        //     let mut last_log = match LAST_LOG.try_lock() {
        //         Ok(guard) => guard,
        //         Err(_) => return, // Skip performance logging if another window is using it
        //     };
        //     if last_log.is_none() {
        //         *last_log = Some(std::time::Instant::now());
        //     }

        //     if let (Some(ref mut counters_map), Some(ref mut last_log_time)) =
        //         (counters.as_mut(), last_log.as_mut())
        //     {
        //         let entry = counters_map
        //             .entry(window_id)
        //             .or_insert((0, std::time::Instant::now()));
        //         entry.0 += 1;

        //         // Log performance stats every 5 seconds
        //         if last_log_time.elapsed().as_secs() >= 5 {
        //             let total_windows = counters_map.len();
        //             if total_windows > 1 {
        //                 tracing::warn!(
        //                     "[PERF] Multi-window performance stats ({} windows):",
        //                     total_windows
        //                 );
        //                 let mut sorted_windows: Vec<_> = counters_map.iter().collect();
        //                 sorted_windows.sort_by(|a, b| b.1 .0.cmp(&a.1 .0)); // Sort by frame count descending

        //                 for (i, (id, (frames, start_time))) in
        //                     sorted_windows.iter().enumerate()
        //                 {
        //                     let fps = *frames as f64 / start_time.elapsed().as_secs_f64();
        //                     let priority = if i == 0 { "HIGH" } else { "LOW" };
        //                     tracing::warn!(
        //                         "[PERF]   Window {:?}: {:.1} FPS ({} frames) [{}]",
        //                         id,
        //                         fps,
        //                         frames,
        //                         priority
        //                     );
        //                 }

        //                 // Check for significant FPS differences
        //                 if sorted_windows.len() >= 2 {
        //                     let highest_fps = sorted_windows[0].1 .0 as f64
        //                         / sorted_windows[0].1 .1.elapsed().as_secs_f64();
        //                     let lowest_fps = sorted_windows.last().unwrap().1 .0 as f64
        //                         / sorted_windows
        //                             .last()
        //                             .unwrap()
        //                             .1
        //                              .1
        //                             .elapsed()
        //                             .as_secs_f64();
        //                     if highest_fps > lowest_fps * 2.0 {
        //                         tracing::error!("[PERF] SIGNIFICANT FPS DIFFERENCE: {:.1} vs {:.1} FPS - window prioritization detected!", highest_fps, lowest_fps);
        //                     }
        //                 }
        //             }
        //             **last_log_time = std::time::Instant::now();
        //             // Reset counters
        //             for (_, (frames, start_time)) in counters_map.iter_mut() {
        //                 *frames = 0;
        //                 *start_time = std::time::Instant::now();
        //             }
        //         }
        //     }
        // }
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
    current_tab_id: u64,
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
            current_tab_id: 0,
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
            let id = self.current_tab_id;
            self.current_tab_id = self.current_tab_id.wrapping_add(1);
            Some(id.to_string())
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
    pub needs_render_after_occlusion: bool,
    pub render_timestamp: Instant,
    #[cfg_attr(target_os = "macos", allow(dead_code))]
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

    pub fn wait_until(&self) -> Option<Duration> {
        // If we need to render after occlusion, render immediately
        if self.needs_render_after_occlusion {
            return None;
        }

        // On macOS, CVDisplayLink handles VSync synchronization automatically,
        // so we don't need software-based frame timing calculations
        #[cfg(target_os = "macos")]
        {
            None
        }

        #[cfg(not(target_os = "macos"))]
        {
            let now = Instant::now();
            let elapsed = now.duration_since(self.render_timestamp);
            let vblank = self.vblank_interval;

            // Calculate how many complete frames have elapsed
            let frames_elapsed = elapsed.as_nanos() / vblank.as_nanos();

            // Calculate when the next frame should occur
            let next_frame_time = self.render_timestamp
                + Duration::from_nanos(
                    (frames_elapsed + 1) as u64 * vblank.as_nanos() as u64,
                );

            if next_frame_time > now {
                // Return the time to wait until the next ideal frame time
                Some(next_frame_time.duration_since(now))
            } else {
                // We've missed the target frame time, render immediately
                None
            }
        }
    }

    // TODO: Use it whenever animated cursor is done
    // pub fn request_animation_frame(&mut self) {
    //     if self.config.renderer.strategy.is_event_based() {
    //         // Schedule a render for the next frame time
    //         let route_id = self.window.screen.ctx().current_route();
    //         let timer_id = TimerId::new(Topic::RenderRoute, route_id);
    //         let event = EventPayload::new(
    //             RioEventType::Rio(RioEvent::RenderRoute(route_id)),
    //             self.window.winit_window.id(),
    //         );

    //         // Always schedule at the next vblank interval
    //         self.scheduler.schedule(event, self.window.vblank_interval, false, timer_id);
    //     } else {
    //         // For game loop rendering, the standard redraw is fine
    //         self.request_redraw();
    //     }
    // }

    #[inline]
    pub fn update_vblank_interval(&mut self) {
        // On macOS, CVDisplayLink handles VSync synchronization automatically,
        // so we don't need to calculate vblank intervals
        #[cfg(not(target_os = "macos"))]
        {
            // Always update vblank interval based on monitor refresh rate
            // Get the display refresh rate, default to 60Hz if unavailable
            let refresh_rate_hz = self
                .winit_window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64
                / 1000.0; // Convert millihertz to Hz

            // Calculate frame time in microseconds (1,000,000 µs / refresh_rate)
            let frame_time_us = (1_000_000.0 / refresh_rate_hz) as u64;
            self.vblank_interval = Duration::from_micros(frame_time_us);
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

        // Get the display refresh rate and convert to frame interval
        // On macOS, CVDisplayLink handles VSync synchronization automatically,
        // so we don't need to calculate vblank intervals
        #[cfg(target_os = "macos")]
        let monitor_vblank_interval = Duration::from_micros(16667); // Placeholder value, not used

        #[cfg(not(target_os = "macos"))]
        let monitor_vblank_interval = {
            let monitor_refresh_rate_hz = winit_window
                .current_monitor()
                .and_then(|monitor| monitor.refresh_rate_millihertz())
                .unwrap_or(60_000) as f64
                / 1000.0;

            // Convert to microseconds for precise frame timing
            let frame_time_us = (1_000_000.0 / monitor_refresh_rate_hz) as u64;
            Duration::from_micros(frame_time_us)
        };

        Self {
            vblank_interval: monitor_vblank_interval,
            render_timestamp: Instant::now(),
            is_focused: true,
            is_occluded: false,
            needs_render_after_occlusion: false,
            winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        }
    }
}
