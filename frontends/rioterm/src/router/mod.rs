pub mod routes;
mod window;
use crate::event::EventProxy;
use crate::router::window::{
    configure_window, create_window_builder, DEFAULT_MINIMUM_WINDOW_HEIGHT,
    DEFAULT_MINIMUM_WINDOW_WIDTH,
};
use crate::screen::{Screen, ScreenWindowProperties};
use assistant::Assistant;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use rio_backend::clipboard::Clipboard;
use rio_backend::config::Config as RioConfig;
use rio_backend::error::{RioError, RioErrorLevel, RioErrorType};

use rio_window::dpi::PhysicalSize;
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
        self.window
            .screen
            .renderer
            .assistant
            .set_error(error.to_owned());
    }

    #[inline]
    pub fn clear_errors(&mut self) {
        self.assistant.clear();
        self.window.screen.renderer.assistant.clear();
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
        use rio_window::event::ElementState;

        // Handle island color picker / rename input
        if let Some(ref mut island) = self.window.screen.renderer.island {
            if island.is_color_picker_open() {
                let consumed = island.handle_rename_input(key_event);
                if consumed {
                    self.window.screen.render();
                    return true;
                }
            }
        }

        // Handle command palette input first (works in all routes)
        if self.window.screen.renderer.command_palette.is_enabled() {
            if key_event.state == ElementState::Pressed {
                match &key_event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .set_enabled(false);
                        self.window.screen.render();
                    }
                    Key::Named(NamedKey::ArrowUp) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .move_selection_up();
                        self.window.screen.render();
                    }
                    Key::Named(NamedKey::ArrowDown) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .move_selection_down();
                        self.window.screen.render();
                    }
                    Key::Named(NamedKey::Tab) => {
                        self.window
                            .screen
                            .renderer
                            .command_palette
                            .move_selection_down();
                        self.window.screen.render();
                    }
                    Key::Named(NamedKey::Enter) => {
                        if let Some(action) = self
                            .window
                            .screen
                            .renderer
                            .command_palette
                            .get_selected_action()
                        {
                            self.window
                                .screen
                                .renderer
                                .command_palette
                                .set_enabled(false);
                            self.window.screen.execute_palette_action(action);
                        }
                        self.window.screen.render();
                    }
                    Key::Named(NamedKey::Backspace) => {
                        let current_query =
                            self.window.screen.renderer.command_palette.query.clone();
                        if !current_query.is_empty() {
                            let mut chars = current_query.chars().collect::<Vec<_>>();
                            chars.pop();
                            self.window
                                .screen
                                .renderer
                                .command_palette
                                .set_query(chars.into_iter().collect());
                            self.window.screen.render();
                        }
                    }
                    _ => {
                        if let Some(text) = key_event.text.as_ref() {
                            // Filter out control characters
                            let text_str = text.as_str();
                            if !text_str.is_empty()
                                && text_str.chars().all(|c| !c.is_control())
                            {
                                let current_query = self
                                    .window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .query
                                    .clone();
                                self.window
                                    .screen
                                    .renderer
                                    .command_palette
                                    .set_query(format!("{}{}", current_query, text_str));
                                self.window.screen.render();
                            }
                        }
                    }
                }
            }
            return true; // Block all input when command palette is active
        }

        if self.path == RoutePath::Terminal {
            return false;
        }

        let is_enter = key_event.logical_key == Key::Named(NamedKey::Enter);

        // Handle assistant overlay dismiss
        if self.window.screen.renderer.assistant.is_active() {
            if is_enter {
                self.assistant.clear();
                self.window.screen.renderer.assistant.clear();
                self.window.screen.render();
            }
            return true;
        }

        if self.path == RoutePath::ConfirmQuit {
            if key_event.state == rio_window::event::ElementState::Pressed {
                match &key_event.logical_key {
                    Key::Character(c) if c.as_str() == "n" || c.as_str() == "N" => {
                        self.path = RoutePath::Terminal;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.path = RoutePath::Terminal;
                    }
                    Key::Character(c) if c.as_str() == "y" || c.as_str() == "Y" => {
                        self.quit();
                        return true;
                    }
                    _ => {}
                }
            }
            return true;
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
            None,
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
        app_id: Option<&str>,
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
            app_id,
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
            None,
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
        app_id: Option<&str>,
    ) -> RouteWindow<'a> {
        #[allow(unused_mut)]
        let mut window_builder =
            create_window_builder(window_name, config, tab_id, app_id);

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

        if let Some((physical_width, physical_height)) =
            compute_startup_window_physical_size(config, screen.ctx().current().dimension)
        {
            let _ = winit_window.request_inner_size(PhysicalSize {
                width: physical_width,
                height: physical_height,
            });
        }

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
        }
    }
}

/// Sum of logical padding and margin on one axis, scaled to physical pixels.
fn scaled_panel_edge(
    padding_start: f32,
    padding_end: f32,
    margin_start: f32,
    margin_end: f32,
    scale: f32,
) -> f32 {
    (padding_start + padding_end + margin_start + margin_end) * scale
}

fn compute_startup_window_physical_size(
    config: &RioConfig,
    dim: crate::layout::ContextDimension,
) -> Option<(u32, u32)> {
    if config.window.columns.is_none() && config.window.rows.is_none() {
        return None;
    }

    let scale = dim.dimension.scale;

    // On Retina (HiDPI) displays, macOS snaps window sizes to multiples of
    // the scale factor (e.g. 2 on 2x displays). Using PhysicalSize and
    // rounding up to the nearest multiple of scale ensures we never end up
    // one physical pixel short, which would cause the renderer to truncate
    // one column or row.
    let scale_u32 = scale.round().max(1.0) as u32;

    let mut physical_width = (config.window.width as f32 * scale).round() as u32;
    let mut physical_height = (config.window.height as f32 * scale).round() as u32;

    // Taffy reserves `config.panel` padding and margin inside the window margin.
    // Startup sizing must include that frame or the first layout reports fewer
    // cols/rows than requested.
    let panel_horizontal = scaled_panel_edge(
        config.panel.padding.left,
        config.panel.padding.right,
        config.panel.margin.left,
        config.panel.margin.right,
        scale,
    );
    let panel_vertical = scaled_panel_edge(
        config.panel.padding.top,
        config.panel.padding.bottom,
        config.panel.margin.top,
        config.panel.margin.bottom,
        scale,
    );

    if let Some(columns) = config.window.columns.filter(|columns| *columns > 0) {
        let margin_horizontal = (dim.margin.left + dim.margin.right) * scale;
        let raw = (columns as f32 * dim.dimension.width).ceil() as u32
            + margin_horizontal as u32
            + panel_horizontal as u32;
        // Round up to nearest multiple of scale factor so macOS Retina
        // snapping never drops us below the target column count.
        physical_width = raw.next_multiple_of(scale_u32);
    }

    if let Some(rows) = config.window.rows.filter(|rows| *rows > 0) {
        let margin_vertical = (dim.margin.top + dim.margin.bottom) * scale;
        let raw = (rows as f32 * dim.dimension.height).ceil() as u32
            + margin_vertical as u32
            + panel_vertical as u32;
        physical_height = raw.next_multiple_of(scale_u32);
    }

    // Keep startup sizing aligned with the global minimum window constraints.
    // These constants are defined in logical pixels, so we convert them to
    // physical pixels using the current scale before clamping.
    let minimum_physical_width =
        (DEFAULT_MINIMUM_WINDOW_WIDTH as f32 * scale).ceil() as u32;
    let minimum_physical_height =
        (DEFAULT_MINIMUM_WINDOW_HEIGHT as f32 * scale).ceil() as u32;

    physical_width = physical_width.max(minimum_physical_width);
    physical_height = physical_height.max(minimum_physical_height);

    Some((physical_width, physical_height))
}

#[test]
fn startup_window_size_returns_none_without_overrides() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.columns = None;
    config.window.rows = None;

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 10.0,
        height: 20.0,
        scale: 2.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(compute_startup_window_physical_size(&config, dim), None);
}

#[test]
fn startup_window_size_applies_only_columns_override() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.width = 500;
    config.window.height = 300;
    config.window.columns = Some(80);
    config.window.rows = None;
    config.panel.padding = Margin::all(0.0);
    config.panel.margin = Margin::all(0.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 10.0,
        height: 20.0,
        scale: 2.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((800, 600))
    );
}

#[test]
fn startup_window_size_applies_only_rows_override() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.width = 500;
    config.window.height = 300;
    config.window.columns = None;
    config.window.rows = Some(24);
    config.panel.padding = Margin::all(0.0);
    config.panel.margin = Margin::all(0.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 10.0,
        height: 20.0,
        scale: 2.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((1000, 480))
    );
}

#[test]
fn startup_window_size_applies_both_overrides() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.columns = Some(100);
    config.window.rows = Some(40);
    config.panel.padding = Margin::all(0.0);
    config.panel.margin = Margin::all(0.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 10.0,
        height: 20.0,
        scale: 1.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((1000, 800))
    );
}

#[test]
fn startup_window_size_ignores_zero_overrides_and_falls_back() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.width = 500;
    config.window.height = 300;
    config.window.columns = Some(0);
    config.window.rows = Some(0);
    config.panel.padding = Margin::all(0.0);
    config.panel.margin = Margin::all(0.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 10.0,
        height: 20.0,
        scale: 2.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((1000, 600))
    );
}

#[test]
fn startup_window_size_rounds_up_on_hidpi() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.columns = Some(80);
    config.window.rows = Some(24);
    config.panel.padding = Margin::all(0.0);
    config.panel.margin = Margin::all(0.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 16.41,
        height: 33.0,
        scale: 2.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((1314, 792))
    );
}

#[test]
fn startup_window_size_includes_terminal_and_panel_margins() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.columns = Some(10);
    config.window.rows = Some(5);
    config.panel.padding = Margin::new(3.0, 2.0, 4.0, 1.0);
    config.panel.margin = Margin::new(7.0, 6.0, 8.0, 5.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 10.0,
        height: 20.0,
        scale: 1.0,
    };
    dim.margin = Margin::new(4.0, 3.0, 5.0, 2.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((300, 200))
    );
}

#[test]
fn startup_window_size_never_goes_under_minimum() {
    use rio_backend::config::layout::Margin;
    use rio_backend::sugarloaf::layout::TextDimensions;

    let mut config = RioConfig::default();
    config.window.width = 50;
    config.window.height = 50;
    config.window.columns = Some(1);
    config.window.rows = Some(1);
    config.panel.padding = Margin::all(0.0);
    config.panel.margin = Margin::all(0.0);

    let mut dim = crate::layout::ContextDimension::default();
    dim.dimension = TextDimensions {
        width: 1.0,
        height: 1.0,
        scale: 1.0,
    };
    dim.margin = Margin::all(0.0);

    assert_eq!(
        compute_startup_window_physical_size(&config, dim),
        Some((300, 200))
    );
}
