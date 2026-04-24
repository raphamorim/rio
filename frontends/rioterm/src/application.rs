use crate::event::{ClickState, EventPayload, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::renderer::utils::update_colors_based_on_theme;
use crate::router::{routes::RoutePath, Router};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::touch::on_touch;
use crate::watcher::configuration_file_updates;
#[cfg(all(
    feature = "audio",
    not(target_os = "macos"),
    not(target_os = "windows")
))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use raw_window_handle::HasDisplayHandle;
use rio_backend::clipboard::{Clipboard, ClipboardType};
use rio_backend::config::colors::{ColorRgb, NamedColor};
use rio_window::application::ApplicationHandler;
use rio_window::event::{
    ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, TouchPhase, WindowEvent,
};
use rio_window::event_loop::ActiveEventLoop;
use rio_window::event_loop::ControlFlow;
use rio_window::event_loop::{DeviceEvents, EventLoop};
#[cfg(target_os = "macos")]
use rio_window::platform::macos::ActiveEventLoopExtMacOS;
#[cfg(target_os = "macos")]
use rio_window::platform::macos::WindowExtMacOS;
use rio_window::window::WindowId;
use rio_window::window::{CursorIcon, Fullscreen};
use std::error::Error;
use std::time::{Duration, Instant};

pub struct Application<'a> {
    config: rio_backend::config::Config,
    event_proxy: EventProxy,
    router: Router<'a>,
    scheduler: Scheduler,
    app_id: Option<String>,
}

impl Application<'_> {
    pub fn new<'app>(
        config: rio_backend::config::Config,
        config_error: Option<rio_backend::config::ConfigError>,
        event_loop: &EventLoop<EventPayload>,
        app_id: Option<String>,
    ) -> Application<'app> {
        // SAFETY: Since this takes a pointer to the winit event loop, it MUST be dropped first,
        // which is done in `exiting`.
        let clipboard =
            unsafe { Clipboard::new(event_loop.display_handle().unwrap().as_raw()) };

        let mut router = Router::new(config.fonts.to_owned(), clipboard);
        if let Some(error) = config_error {
            router.propagate_error_to_next_route(error.into());
        }

        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let _ = configuration_file_updates(
            rio_backend::config::config_dir_path(),
            event_proxy.clone(),
        );
        let scheduler = Scheduler::new(proxy);
        event_loop.listen_device_events(DeviceEvents::Never);

        #[cfg(any(target_os = "macos", target_os = "windows"))]
        event_loop.set_confirm_before_quit(config.confirm_before_quit);

        rio_notifier::request_authorization();

        Application {
            config,
            event_proxy,
            router,
            scheduler,
            app_id,
        }
    }

    fn skip_window_event(event: &WindowEvent) -> bool {
        matches!(
            event,
            WindowEvent::KeyboardInput {
                is_synthetic: true,
                ..
            } | WindowEvent::ActivationTokenDone { .. }
                | WindowEvent::DoubleTapGesture { .. }
                | WindowEvent::TouchpadPressure { .. }
                | WindowEvent::RotationGesture { .. }
                | WindowEvent::CursorEntered { .. }
                | WindowEvent::PinchGesture { .. }
                | WindowEvent::AxisMotion { .. }
                | WindowEvent::PanGesture { .. }
                | WindowEvent::HoveredFileCancelled
                | WindowEvent::Destroyed
                | WindowEvent::HoveredFile(_)
                | WindowEvent::Moved(_)
        )
    }

    fn handle_audio_bell(&mut self) {
        #[cfg(target_os = "macos")]
        {
            // Use system bell sound on macOS
            unsafe {
                #[link(name = "AppKit", kind = "framework")]
                extern "C" {
                    fn NSBeep();
                }
                NSBeep();
            }
        }

        #[cfg(target_os = "windows")]
        {
            // Use MessageBeep on Windows with MB_OK (0x00000000) for default beep
            unsafe {
                windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(0x00000000);
            }
        }

        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            #[cfg(feature = "audio")]
            {
                std::thread::spawn(|| {
                    if let Err(e) = play_bell_sound() {
                        tracing::warn!("Failed to play bell sound: {}", e);
                    }
                });
            }
            #[cfg(not(feature = "audio"))]
            {
                tracing::debug!("Audio bell requested but audio feature is not enabled");
            }
        }
    }

    fn handle_desktop_notification(&self, title: &str, body: &str) {
        rio_notifier::send_notification(title, body);
    }

    pub fn run(
        &mut self,
        event_loop: EventLoop<EventPayload>,
    ) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        result.map_err(Into::into)
    }
}

impl ApplicationHandler<EventPayload> for Application<'_> {
    fn resumed(&mut self, _active_event_loop: &ActiveEventLoop) {}

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause != StartCause::Init
            && cause != StartCause::CreateWindow
            && cause != StartCause::MacOSReopen
        {
            return;
        }

        if cause == StartCause::MacOSReopen && !self.router.routes.is_empty() {
            return;
        }

        let theme = self
            .config
            .force_theme
            .map(|t| t.to_window_theme())
            .or(event_loop.system_theme());
        update_colors_based_on_theme(&mut self.config, theme);

        self.router.create_window(
            event_loop,
            self.event_proxy.clone(),
            &self.config,
            None,
            self.app_id.as_deref(),
        );

        // Schedule title updates every 2s
        let timer_id = TimerId::new(Topic::UpdateTitles, 0);
        if !self.scheduler.scheduled(timer_id) {
            self.scheduler.schedule(
                EventPayload::new(RioEventType::Rio(RioEvent::UpdateTitles), unsafe {
                    rio_window::window::WindowId::dummy()
                }),
                Duration::from_secs(2),
                true,
                timer_id,
            );
        }

        tracing::info!("Initialisation complete");
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EventPayload) {
        let window_id = event.window_id;
        match event.payload {
            RioEventType::Rio(RioEvent::Render) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // Skip rendering for unfocused windows if configured
                    if self.config.renderer.disable_unfocused_render
                        && !route.window.is_focused
                    {
                        return;
                    }

                    // Skip rendering for occluded windows if configured, unless we need to render after occlusion
                    if self.config.renderer.disable_occluded_render
                        && route.window.is_occluded
                        && !route.window.needs_render_after_occlusion
                    {
                        return;
                    }

                    // Clear the one-time render flag if it was set
                    if route.window.needs_render_after_occlusion {
                        route.window.needs_render_after_occlusion = false;
                    }

                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::RenderRoute(route_id)) => {
                if self.config.renderer.strategy.is_event_based() {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        // Skip rendering for unfocused windows if configured
                        if self.config.renderer.disable_unfocused_render
                            && !route.window.is_focused
                        {
                            return;
                        }

                        // Skip rendering for occluded windows if configured, unless we need to render after occlusion
                        if self.config.renderer.disable_occluded_render
                            && route.window.is_occluded
                            && !route.window.needs_render_after_occlusion
                        {
                            return;
                        }

                        // Clear the one-time render flag if it was set
                        if route.window.needs_render_after_occlusion {
                            route.window.needs_render_after_occlusion = false;
                        }

                        // Mark the renderable content as needing to render
                        if let Some(ctx_item) =
                            route.window.screen.ctx_mut().get_by_route_id(route_id)
                        {
                            ctx_item.val.renderable_content.pending_update.set_dirty();
                        }

                        // Check if we need to throttle based on timing
                        if let Some(wait_duration) = route.window.wait_until() {
                            // We need to wait before rendering again
                            let timer_id = TimerId::new(Topic::RenderRoute, route_id);
                            let event = EventPayload::new(
                                RioEventType::Rio(RioEvent::Render),
                                window_id,
                            );

                            // Only schedule if not already scheduled
                            if !self.scheduler.scheduled(timer_id) {
                                self.scheduler.schedule(
                                    event,
                                    wait_duration,
                                    false,
                                    timer_id,
                                );
                            }
                        } else {
                            // We can render immediately
                            route.request_redraw();
                        }
                    }
                }
            }

            RioEventType::Rio(RioEvent::TerminalDamaged(route_id)) => {
                if self.config.renderer.strategy.is_event_based() {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if self.config.renderer.disable_unfocused_render
                            && !route.window.is_focused
                        {
                            return;
                        }
                        if self.config.renderer.disable_occluded_render
                            && route.window.is_occluded
                            && !route.window.needs_render_after_occlusion
                        {
                            return;
                        }

                        if let Some(ctx_item) =
                            route.window.screen.ctx_mut().get_by_route_id(route_id)
                        {
                            // Just mark dirty — damage will be extracted from
                            // the terminal when the renderer locks it.
                            ctx_item.val.renderable_content.pending_update.set_dirty();
                            route.schedule_redraw(&mut self.scheduler, route_id);
                        }
                    }
                }
            }
            RioEventType::Rio(RioEvent::UpdateGraphics { route_id, queues }) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    // Process graphics directly in sugarloaf
                    let sugarloaf = &mut route.window.screen.sugarloaf;

                    // Atlas graphics (sixel/iTerm2)
                    for graphic_data in queues.pending {
                        sugarloaf.graphics.insert(graphic_data);
                    }

                    // Image textures (kitty) → separate store, no clone
                    for (image_id, graphic_data) in queues.pending_images {
                        sugarloaf.image_data.insert(
                            image_id,
                            rio_backend::sugarloaf::GraphicDataEntry::from_graphic_data(
                                graphic_data,
                            ),
                        );
                    }

                    for graphic_data in queues.remove_queue {
                        sugarloaf.graphics.remove(&graphic_data);
                    }

                    // Request a redraw to display the updated graphics
                    route.schedule_redraw(&mut self.scheduler, route_id);
                }
            }
            RioEventType::Rio(RioEvent::PrepareUpdateConfig) => {
                let timer_id = TimerId::new(Topic::UpdateConfig, 0);
                let event = EventPayload::new(
                    RioEventType::Rio(RioEvent::UpdateConfig),
                    window_id,
                );

                if !self.scheduler.scheduled(timer_id) {
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(250),
                        false,
                        timer_id,
                    );
                }
            }
            RioEventType::Rio(RioEvent::ReportToAssistant(error)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.report_error(&error);
                }
            }
            RioEventType::Rio(RioEvent::UpdateConfig) => {
                let (config, config_error) = match rio_backend::config::Config::try_load()
                {
                    Ok(config) => (config, None),
                    Err(error) => (rio_backend::config::Config::default(), Some(error)),
                };

                let has_font_updates = self.config.fonts != config.fonts;

                let font_library_errors = if has_font_updates {
                    let new_font_library = rio_backend::sugarloaf::font::FontLibrary::new(
                        config.fonts.to_owned(),
                    );
                    *self.router.font_library = new_font_library.0;
                    new_font_library.1
                } else {
                    None
                };

                self.config = config;

                let mut has_checked_adaptive_colors = false;
                for (_id, route) in self.router.routes.iter_mut() {
                    // Apply system theme to ensure colors are consistent
                    if !has_checked_adaptive_colors {
                        let system_theme = route.window.winit_window.theme();
                        let theme = self
                            .config
                            .force_theme
                            .map(|t| t.to_window_theme())
                            .or(system_theme);
                        update_colors_based_on_theme(&mut self.config, theme);
                        has_checked_adaptive_colors = true;
                    }

                    if has_font_updates {
                        if let Some(ref err) = font_library_errors {
                            route
                                .window
                                .screen
                                .context_manager
                                .report_error_fonts_not_found(
                                    err.fonts_not_found.clone(),
                                );
                        }
                    }

                    route.update_config(
                        &self.config,
                        &self.router.font_library,
                        has_font_updates,
                    );
                    route.window.configure_window(&self.config);

                    if let Some(error) = &config_error {
                        route.report_error(&error.to_owned().into());
                    } else {
                        route.clear_errors();
                    }
                }
            }
            RioEventType::Rio(RioEvent::Exit) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if self.config.confirm_before_quit {
                        route.confirm_quit();
                        route.request_redraw();
                    } else {
                        route.quit();
                    }
                }
            }
            RioEventType::Rio(RioEvent::CloseTerminal(route_id)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route
                        .window
                        .screen
                        .context_manager
                        .should_close_context_manager(
                            route_id,
                            &mut route.window.screen.sugarloaf,
                        )
                    {
                        self.router.routes.remove(&window_id);

                        // Unschedule pending events.
                        self.scheduler.unschedule_window(route_id);

                        if self.router.routes.is_empty() {
                            event_loop.exit();
                        }
                    } else {
                        let size = route.window.screen.context_manager.len();
                        route.window.screen.resize_top_or_bottom_line(size);
                    }
                }
            }
            RioEventType::Rio(RioEvent::CursorBlinkingChange) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::CursorBlinkingChangeOnRoute(route_id)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route_id == route.window.screen.ctx().current_route() {
                        // Get cursor position for damage
                        let cursor_line = {
                            let terminal = route
                                .window
                                .screen
                                .ctx_mut()
                                .current_mut()
                                .terminal
                                .lock();
                            terminal.cursor().pos.row.0 as usize
                        };

                        // Set terminal damage for cursor line
                        route
                            .window
                            .screen
                            .ctx_mut()
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_terminal_damage(
                                rio_backend::event::TerminalDamage::Partial(
                                    [rio_backend::crosswords::LineDamage::new(
                                        cursor_line,
                                        true,
                                    )]
                                    .into_iter()
                                    .collect(),
                                ),
                            );

                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::ProgressReport(report)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if let Some(island) = &mut route.window.screen.renderer.island {
                        island.set_progress_report(report);
                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::SelectionScrollTick) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.screen.selection_scroll_tick();
                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::Bell) => {
                // Handle audio bell
                if self.config.bell.audio {
                    self.handle_audio_bell();
                }
            }
            RioEventType::Rio(RioEvent::DesktopNotification { title, body }) => {
                self.handle_desktop_notification(&title, &body);
            }
            RioEventType::Rio(RioEvent::PrepareRender(millis)) => {
                if let Some(route) = self.router.routes.get(&window_id) {
                    let timer_id = TimerId::new(
                        Topic::Render,
                        route.window.screen.ctx().current_route(),
                    );
                    let event =
                        EventPayload::new(RioEventType::Rio(RioEvent::Render), window_id);

                    if !self.scheduler.scheduled(timer_id) {
                        self.scheduler.schedule(
                            event,
                            Duration::from_millis(millis),
                            false,
                            timer_id,
                        );
                    }
                }
            }
            RioEventType::Rio(RioEvent::PrepareRenderOnRoute(millis, route_id)) => {
                let timer_id = TimerId::new(Topic::RenderRoute, route_id);
                let event = EventPayload::new(
                    RioEventType::Rio(RioEvent::RenderRoute(route_id)),
                    window_id,
                );

                if !self.scheduler.scheduled(timer_id) {
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(millis),
                        false,
                        timer_id,
                    );
                }
            }
            RioEventType::Rio(RioEvent::BlinkCursor(millis, route_id)) => {
                let timer_id = TimerId::new(Topic::CursorBlinking, route_id);
                let event = EventPayload::new(
                    RioEventType::Rio(RioEvent::CursorBlinkingChangeOnRoute(route_id)),
                    window_id,
                );

                if !self.scheduler.scheduled(timer_id) {
                    self.scheduler.schedule(
                        event,
                        Duration::from_millis(millis),
                        false,
                        timer_id,
                    );
                }
            }
            RioEventType::Rio(RioEvent::Title(title)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.set_window_title(&title);
                }
            }
            RioEventType::Rio(RioEvent::TitleWithSubtitle(title, subtitle)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.set_window_title(&title);
                    route.set_window_subtitle(&subtitle);
                }
            }
            RioEventType::Rio(RioEvent::UpdateTitles) => {
                self.router.update_titles();
            }
            RioEventType::Rio(RioEvent::MouseCursorDirty) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.screen.reset_mouse();
                }
            }
            RioEventType::Rio(RioEvent::Scroll(scroll)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let mut terminal = route
                        .window
                        .screen
                        .context_manager
                        .current_mut()
                        .terminal
                        .lock();
                    terminal.scroll_display(scroll);
                    drop(terminal);
                }
            }
            RioEventType::Rio(RioEvent::ClipboardLoad(clipboard_type, format)) => {
                let Router {
                    routes, clipboard, ..
                } = &mut self.router;
                if let Some(route) = routes.get_mut(&window_id) {
                    if route.window.is_focused {
                        let text = format(clipboard.get(clipboard_type).as_str());
                        route
                            .window
                            .screen
                            .ctx_mut()
                            .current_mut()
                            .messenger
                            .send_bytes(text.into_bytes());
                    }
                }
            }
            RioEventType::Rio(RioEvent::ClipboardStore(clipboard_type, content)) => {
                let Router {
                    routes, clipboard, ..
                } = &mut self.router;
                if let Some(route) = routes.get_mut(&window_id) {
                    if route.window.is_focused {
                        clipboard.set(clipboard_type, content);
                    }
                }
            }
            RioEventType::Rio(RioEvent::PtyWrite(text)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route
                        .window
                        .screen
                        .ctx_mut()
                        .current_mut()
                        .messenger
                        .send_bytes(text.into_bytes());
                }
            }
            RioEventType::Rio(RioEvent::TextAreaSizeRequest(format)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let dimension =
                        route.window.screen.context_manager.current().dimension;
                    let text =
                        format(crate::renderer::utils::terminal_dimensions(&dimension));
                    route
                        .window
                        .screen
                        .ctx_mut()
                        .current_mut()
                        .messenger
                        .send_bytes(text.into_bytes());
                }
            }
            RioEventType::Rio(RioEvent::ColorRequest(index, format)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let terminal = route
                        .window
                        .screen
                        .context_manager
                        .current()
                        .terminal
                        .lock();
                    let color: ColorRgb = match terminal.colors()[index] {
                        Some(color) => ColorRgb::from_color_arr(color),
                        // Ignore cursor color requests unless it was changed.
                        None if index
                            == crate::crosswords::NamedColor::Cursor as usize =>
                        {
                            return
                        }
                        None => ColorRgb::from_color_arr(
                            route.window.screen.renderer.colors[index],
                        ),
                    };

                    drop(terminal);

                    route
                        .window
                        .screen
                        .ctx_mut()
                        .current_mut()
                        .messenger
                        .send_bytes(format(color).into_bytes());
                }
            }
            RioEventType::Rio(RioEvent::CreateWindow) => {
                self.router.create_window(
                    event_loop,
                    self.event_proxy.clone(),
                    &self.config,
                    None,
                    self.app_id.as_deref(),
                );
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::CreateNativeTab(working_dir_overwrite)) => {
                if let Some(route) = self.router.routes.get(&window_id) {
                    // This case happens only for native tabs
                    // every time that a new tab is created through context
                    // it also reaches for the foreground process path if
                    // config.use_current_path is true
                    // For these case we need to make a workaround
                    let config = if working_dir_overwrite.is_some() {
                        rio_backend::config::Config {
                            working_dir: working_dir_overwrite,
                            ..self.config.clone()
                        }
                    } else {
                        self.config.clone()
                    };

                    self.router.create_native_tab(
                        event_loop,
                        self.event_proxy.clone(),
                        &config,
                        Some(&route.window.winit_window.tabbing_identifier()),
                        None,
                    );
                }
            }
            RioEventType::Rio(RioEvent::CreateConfigEditor) => {
                if self.config.navigation.open_config_with_split {
                    self.router.open_config_split(&self.config);
                } else {
                    self.router.open_config_window(
                        event_loop,
                        self.event_proxy.clone(),
                        &self.config,
                    );
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::CloseWindow) => {
                self.router.routes.remove(&window_id);
                if self.router.routes.is_empty() && !self.config.confirm_before_quit {
                    event_loop.exit();
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabByIndex(tab_index)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.select_tab_at_index(tab_index);
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabLast) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route
                        .window
                        .winit_window
                        .select_tab_at_index(route.window.winit_window.num_tabs() - 1);
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabNext) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.select_next_tab();
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::SelectNativeTabPrev) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.select_previous_tab();
                }
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::Hide) => {
                event_loop.hide_application();
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::HideOtherApplications) => {
                event_loop.hide_other_applications();
            }
            RioEventType::Rio(RioEvent::Minimize(set_minimize)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.winit_window.set_minimized(set_minimize);
                }
            }
            RioEventType::Rio(RioEvent::ToggleFullScreen) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    match route.window.winit_window.fullscreen() {
                        None => route
                            .window
                            .winit_window
                            .set_fullscreen(Some(Fullscreen::Borderless(None))),
                        _ => route.window.winit_window.set_fullscreen(None),
                    }
                }
            }
            RioEventType::Rio(RioEvent::ToggleAppearanceTheme) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    use rio_backend::config::theme::AppearanceTheme;
                    let current = self
                        .config
                        .force_theme
                        .or_else(|| {
                            route
                                .window
                                .winit_window
                                .theme()
                                .map(AppearanceTheme::from_window_theme)
                        })
                        .unwrap_or(AppearanceTheme::Dark);
                    let toggled = current.toggled();
                    self.config.force_theme = Some(toggled);
                    update_colors_based_on_theme(
                        &mut self.config,
                        Some(toggled.to_window_theme()),
                    );
                    route.window.screen.update_config(
                        &self.config,
                        &self.router.font_library,
                        false,
                    );
                    route.window.configure_window(&self.config);
                }
            }
            RioEventType::Rio(RioEvent::ColorChange(route_id, index, color)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let screen = &mut route.window.screen;
                    // Background color is index 1 relative to NamedColor::Foreground
                    if index == NamedColor::Foreground as usize + 1 {
                        let grid = screen.context_manager.current_grid_mut();
                        // The event carries a `route_id: usize` (global
                        // counter). `ContextGrid::get_mut` is keyed on
                        // taffy `NodeId` — a different identifier space,
                        // so `get_mut(route_id.into())` effectively
                        // never matches. Look the panel up by its
                        // actual route id.
                        if let Some(context_item) = grid.get_by_route_id(route_id) {
                            use crate::context::renderable::BackgroundState;
                            context_item.context_mut().renderable_content.background =
                                Some(match color {
                                    Some(c) => BackgroundState::Set(c.to_wgpu()),
                                    None => BackgroundState::Reset,
                                });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    #[cfg(target_os = "macos")]
    fn open_urls(&mut self, active_event_loop: &ActiveEventLoop, urls: Vec<String>) {
        if !self.config.navigation.is_native() {
            let config = &self.config;
            for url in urls {
                self.router.create_window(
                    active_event_loop,
                    self.event_proxy.clone(),
                    config,
                    Some(url),
                    self.app_id.as_deref(),
                );
            }
            return;
        }

        let mut tab_id = None;

        // In case only have one window
        for (_, route) in self.router.routes.iter() {
            if tab_id.is_none() {
                tab_id = Some(route.window.winit_window.tabbing_identifier());
            }

            if route.window.is_focused {
                tab_id = Some(route.window.winit_window.tabbing_identifier());
                break;
            }
        }

        if tab_id.is_some() {
            let config = &self.config;
            for url in urls {
                self.router.create_native_tab(
                    active_event_loop,
                    self.event_proxy.clone(),
                    config,
                    tab_id.as_deref(),
                    Some(url),
                );
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Ignore all events we do not care about.
        if Self::skip_window_event(&event) {
            return;
        }

        let route = match self.router.routes.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                // macOS: Cmd+Q quit confirmation is handled by
                // `applicationShouldTerminate` in rio-window.
                // Windows: per-window close confirmation is handled
                // by `MessageBoxW` in rio-window's WM_CLOSE handler
                // (see `set_confirm_before_quit` plumbing).
                // Either way, by the time we see `CloseRequested`
                // the user has already confirmed — just close.
                if cfg!(any(target_os = "macos", target_os = "windows")) {
                    self.router.routes.remove(&window_id);
                    if self.router.routes.is_empty() {
                        event_loop.exit();
                    }
                    return;
                }

                if self.config.confirm_before_quit {
                    route.confirm_quit();
                    route.request_redraw();
                    return;
                } else {
                    self.router.routes.remove(&window_id);
                }

                if self.router.routes.is_empty() {
                    event_loop.exit();
                }
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                route.window.screen.set_modifiers(modifiers);
            }

            WindowEvent::MouseInput { state, button, .. } => {
                if route.path != RoutePath::Terminal {
                    return;
                }

                if self.config.hide_cursor_when_typing {
                    route.window.winit_window.set_cursor_visible(true);
                }

                match button {
                    MouseButton::Left => {
                        route.window.screen.mouse.left_button_state = state
                    }
                    MouseButton::Middle => {
                        route.window.screen.mouse.middle_button_state = state
                    }
                    MouseButton::Right => {
                        route.window.screen.mouse.right_button_state = state
                    }
                    _ => (),
                }

                match state {
                    ElementState::Pressed => {
                        // Calculate time since the last click to handle double/triple clicks.
                        // Do this early so island clicks can use the click state
                        let now = Instant::now();
                        let elapsed =
                            now - route.window.screen.mouse.last_click_timestamp;
                        route.window.screen.mouse.last_click_timestamp = now;

                        let threshold = Duration::from_millis(300);
                        let mouse = &route.window.screen.mouse;
                        route.window.screen.mouse.click_state = match mouse.click_state {
                            // Reset click state if button has changed.
                            _ if button != mouse.last_click_button => {
                                route.window.screen.mouse.last_click_button = button;
                                ClickState::Click
                            }
                            ClickState::Click if elapsed < threshold => {
                                ClickState::DoubleClick
                            }
                            ClickState::DoubleClick if elapsed < threshold => {
                                ClickState::TripleClick
                            }
                            _ => ClickState::Click,
                        };

                        if let MouseButton::Left = button {
                            // Check if clicking on a panel border to start resize
                            {
                                let mx = route.window.screen.mouse.x as f32;
                                let my = route.window.screen.mouse.y as f32;
                                let grid =
                                    route.window.screen.context_manager.current_grid();
                                if let Some(border) = grid.find_border_at_position(mx, my)
                                {
                                    let start_pos = match border.direction {
                                        crate::layout::BorderDirection::Vertical => mx,
                                        crate::layout::BorderDirection::Horizontal => my,
                                    };
                                    let size_a = grid.get_panel_size(
                                        border.left_or_top,
                                        border.direction,
                                    );
                                    let size_b = grid.get_panel_size(
                                        border.right_or_bottom,
                                        border.direction,
                                    );
                                    route.window.screen.resize_state =
                                        Some(crate::layout::ResizeState {
                                            border,
                                            start_pos,
                                            original_sizes: (size_a, size_b),
                                        });
                                    return;
                                }
                            }

                            if route.window.screen.handle_assistant_click() {
                                route.request_redraw();
                                return;
                            }

                            if route
                                .window
                                .screen
                                .handle_palette_click(&mut self.router.clipboard)
                            {
                                route.request_redraw();
                                return;
                            }

                            if route
                                .window
                                .screen
                                .handle_search_click(&mut self.router.clipboard)
                            {
                                route.request_redraw();
                                return;
                            }

                            let handled_by_island =
                                route.window.screen.handle_island_click(
                                    &route.window.winit_window,
                                    &mut self.router.clipboard,
                                );

                            if handled_by_island {
                                // Island handled the click, don't process further
                                route.request_redraw();
                                return;
                            }

                            if route.window.screen.handle_scrollbar_click() {
                                route.request_redraw();
                                return;
                            }
                        }

                        // Always try panel switching first: if the click
                        // targets a different panel, switch to it regardless
                        // of mouse mode (e.g. neovim capturing clicks).
                        if route.window.screen.select_current_based_on_mouse() {
                            route.request_redraw();
                        } else if !route.window.screen.modifiers.state().shift_key()
                            && route.window.screen.mouse_mode()
                        {
                            // Process mouse press before bindings to update the `click_state`.
                            route.window.screen.mouse.click_state = ClickState::None;

                            let code = match button {
                                MouseButton::Left => 0,
                                MouseButton::Middle => 1,
                                MouseButton::Right => 2,
                                // Can't properly report more than three buttons..
                                MouseButton::Back
                                | MouseButton::Forward
                                | MouseButton::Other(_) => return,
                            };

                            route
                                .window
                                .screen
                                .mouse_report(code, ElementState::Pressed);

                            route.window.screen.process_mouse_bindings(
                                button,
                                &mut self.router.clipboard,
                            );
                        } else {
                            if route.window.screen.trigger_hyperlink() {
                                return;
                            }

                            // Load mouse point, treating message bar and padding as the closest square.
                            let display_offset = route.window.screen.display_offset();

                            if let MouseButton::Left = button {
                                let pos =
                                    route.window.screen.mouse_position(display_offset);
                                route
                                    .window
                                    .screen
                                    .on_left_click(pos, &mut self.router.clipboard);
                            }

                            route.request_redraw();
                        }
                        route
                            .window
                            .screen
                            .process_mouse_bindings(button, &mut self.router.clipboard);
                    }
                    ElementState::Released => {
                        // Stop selection auto-scroll on button release.
                        if let MouseButton::Left | MouseButton::Right = button {
                            let scroll_timer_id =
                                route.window.screen.ctx().current_route();
                            let timer_id =
                                TimerId::new(Topic::SelectionScrolling, scroll_timer_id);
                            self.scheduler.unschedule(timer_id);
                        }

                        if route.window.screen.renderer.scrollbar.is_dragging() {
                            route.window.screen.handle_scrollbar_release();
                            route.request_redraw();
                            return;
                        }

                        if route.window.screen.resize_state.is_some() {
                            route.window.screen.resize_state = None;
                            route.window.winit_window.set_cursor(CursorIcon::Default);
                            return;
                        }

                        if !route.window.screen.modifiers.state().shift_key()
                            && route.window.screen.mouse_mode()
                        {
                            let code = match button {
                                MouseButton::Left => 0,
                                MouseButton::Middle => 1,
                                MouseButton::Right => 2,
                                // Can't properly report more than three buttons.
                                MouseButton::Back
                                | MouseButton::Forward
                                | MouseButton::Other(_) => return,
                            };
                            route
                                .window
                                .screen
                                .mouse_report(code, ElementState::Released);
                            return;
                        }

                        // Trigger hints highlighted by the mouse
                        if button == MouseButton::Left
                            && route
                                .window
                                .screen
                                .trigger_hint(&mut self.router.clipboard)
                        {
                            return;
                        }

                        if let MouseButton::Left | MouseButton::Right = button {
                            if self.config.copy_on_select {
                                route.window.screen.copy_selection(
                                    ClipboardType::Clipboard,
                                    &mut self.router.clipboard,
                                );
                            }
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                if self.config.hide_cursor_when_typing {
                    route.window.winit_window.set_cursor_visible(true);
                }

                if route.path != RoutePath::Terminal {
                    route.window.winit_window.set_cursor(CursorIcon::Default);
                    return;
                }

                let x = position.x;
                let y = position.y;

                let layout = route.window.screen.sugarloaf.window_size();

                let x = x.clamp(0.0, (layout.width as i32 - 1).into()) as usize;
                let y = y.clamp(0.0, (layout.height as i32 - 1).into()) as usize;

                // Snapshot the old mouse position before updating coordinates
                // so we can detect whether the cursor moved to a new cell.
                let old_x = route.window.screen.mouse.x;
                let old_y = route.window.screen.mouse.y;

                route.window.screen.mouse.x = x;
                route.window.screen.mouse.y = y;
                route.window.screen.mouse.raw_y = position.y;

                // Handle assistant overlay hover
                if route.window.screen.renderer.assistant.is_active() {
                    let scale = route.window.screen.sugarloaf.scale_factor();
                    let win_w = route.window.screen.sugarloaf.window_size().width;
                    let mx = x as f32 / scale;
                    let my = y as f32 / scale;
                    if route
                        .window
                        .screen
                        .renderer
                        .assistant
                        .hover(mx, my, win_w, scale)
                    {
                        route.request_redraw();
                    }

                    if route
                        .window
                        .screen
                        .renderer
                        .assistant
                        .hovered_button()
                        .is_some()
                    {
                        route.window.winit_window.set_cursor(CursorIcon::Pointer);
                    } else {
                        route.window.winit_window.set_cursor(CursorIcon::Default);
                    }
                    return;
                }

                // Handle command palette hover
                if route.window.screen.renderer.command_palette.is_enabled() {
                    let scale = route.window.screen.sugarloaf.scale_factor();
                    let win_w = route.window.screen.sugarloaf.window_size().width;
                    let mx = x as f32 / scale;
                    let my = y as f32 / scale;
                    if route
                        .window
                        .screen
                        .renderer
                        .command_palette
                        .hover(mx, my, win_w, scale)
                    {
                        route.request_redraw();
                    }
                    route.window.winit_window.set_cursor(CursorIcon::Default);
                    return;
                }

                // Handle search overlay hover
                if route.window.screen.renderer.search.is_active() {
                    let scale = route.window.screen.sugarloaf.scale_factor();
                    let win_w = route.window.screen.sugarloaf.window_size().width;
                    let mx = x as f32 / scale;
                    let my = y as f32 / scale;
                    if route
                        .window
                        .screen
                        .renderer
                        .search
                        .hover(mx, my, win_w, scale)
                    {
                        // UI-only change (hover highlight). `set_dirty`
                        // passes `Renderer::run`'s per-context gate;
                        // the inner damage match hits
                        // `(None, None) => TerminalDamage::Noop` so
                        // no rows rebuild. The search overlay itself
                        // is drawn unconditionally after the per-context
                        // loop in `Renderer::run`.
                        route
                            .window
                            .screen
                            .ctx_mut()
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_dirty();
                        route.request_redraw();
                    }
                }

                // Check if mouse is over island and set cursor to default
                use crate::renderer::island::ISLAND_HEIGHT;
                let scale_factor = route.window.screen.sugarloaf.scale_factor();
                let island_height_px = (ISLAND_HEIGHT * scale_factor) as usize;
                if route.window.screen.renderer.navigation.is_enabled()
                    && y <= island_height_px
                {
                    route.window.winit_window.set_cursor(CursorIcon::Default);
                    return;
                }

                // Handle scrollbar drag
                if route.window.screen.renderer.scrollbar.is_dragging() {
                    let scale = route.window.screen.sugarloaf.scale_factor();
                    let mouse_y = y as f32 / scale;
                    route.window.screen.handle_scrollbar_drag(mouse_y);
                    route.window.winit_window.set_cursor(CursorIcon::Default);
                    route.request_redraw();
                    return;
                }

                // Handle panel border resize
                if route.window.screen.resize_state.is_some() {
                    let state = route.window.screen.resize_state.unwrap();
                    let current_pos = match state.border.direction {
                        crate::layout::BorderDirection::Vertical => x as f32,
                        crate::layout::BorderDirection::Horizontal => y as f32,
                    };
                    let delta = current_pos - state.start_pos;
                    let border = state.border;
                    let original_sizes = state.original_sizes;
                    route
                        .window
                        .screen
                        .context_manager
                        .current_grid_mut()
                        .resize_border(
                            &border,
                            original_sizes,
                            delta,
                            &mut route.window.screen.sugarloaf,
                        );
                    let cursor = match border.direction {
                        crate::layout::BorderDirection::Vertical => CursorIcon::ColResize,
                        crate::layout::BorderDirection::Horizontal => {
                            CursorIcon::RowResize
                        }
                    };
                    route.window.winit_window.set_cursor(cursor);
                    route.window.screen.context_manager.request_render();
                    route.request_redraw();
                    return;
                }

                // Check if hovering over a panel border
                {
                    let grid = route.window.screen.context_manager.current_grid();
                    if let Some(border) = grid.find_border_at_position(x as f32, y as f32)
                    {
                        let cursor = match border.direction {
                            crate::layout::BorderDirection::Vertical => {
                                CursorIcon::ColResize
                            }
                            crate::layout::BorderDirection::Horizontal => {
                                CursorIcon::RowResize
                            }
                        };
                        route.window.winit_window.set_cursor(cursor);
                        route.window.screen.mouse.on_border = true;
                        return;
                    }
                }

                // Check if hovering over scrollbar
                if route.window.screen.is_hovering_scrollbar() {
                    route.window.winit_window.set_cursor(CursorIcon::Default);
                    return;
                }

                // Track leaving a border to force cursor reset below
                let was_on_border = route.window.screen.mouse.on_border;
                route.window.screen.mouse.on_border = false;

                let lmb_pressed =
                    route.window.screen.mouse.left_button_state == ElementState::Pressed;
                let rmb_pressed =
                    route.window.screen.mouse.right_button_state == ElementState::Pressed;

                let has_selection = !route.window.screen.selection_is_empty();
                if has_selection && (lmb_pressed || rmb_pressed) {
                    // Only start the timer when the mouse enters the scroll
                    // zone. Once running, the tick reads mouse.raw_y each
                    // iteration so it keeps scrolling after CursorMoved
                    // stops (mouse left window). Cancelled on button release.
                    let delta = route.window.screen.selection_scroll_delta(position.y);
                    if delta != 0 {
                        let scroll_timer_id = route.window.screen.ctx().current_route();
                        let timer_id =
                            TimerId::new(Topic::SelectionScrolling, scroll_timer_id);
                        if !self.scheduler.scheduled(timer_id) {
                            let event = EventPayload::new(
                                RioEventType::Rio(RioEvent::SelectionScrollTick),
                                window_id,
                            );
                            self.scheduler.schedule(
                                event,
                                Duration::from_millis(15),
                                true,
                                timer_id,
                            );
                        }
                    }
                }

                let display_offset = route.window.screen.display_offset();
                let point = route.window.screen.mouse_position(display_offset);

                // Detect cell change by comparing pixel positions against cell
                // dimensions, avoiding a second mouse_position() call.
                let square_changed = x != old_x || y != old_y;

                let inside_text_area = route.window.screen.contains_point(x, y);
                let square_side = route.window.screen.side_by_pos(x);

                // If the mouse hasn't changed cells, do nothing.
                // Force update when transitioning off a border so the cursor resets.
                if !square_changed
                    && !was_on_border
                    && route.window.screen.mouse.square_side == square_side
                    && route.window.screen.mouse.inside_text_area == inside_text_area
                {
                    return;
                }

                // Skip hint/hyperlink highlighting during active selection
                // drag to avoid unnecessary terminal locks and regex matching.
                let is_selecting = (lmb_pressed || rmb_pressed)
                    && (route.window.screen.modifiers.state().shift_key()
                        || !route.window.screen.mouse_mode());

                if !is_selecting && route.window.screen.update_highlighted_hints() {
                    route.window.winit_window.set_cursor(CursorIcon::Pointer);
                    route.window.screen.context_manager.request_render();
                } else if !is_selecting {
                    let cursor_icon =
                        if !route.window.screen.modifiers.state().shift_key()
                            && route.window.screen.mouse_mode()
                        {
                            CursorIcon::Default
                        } else {
                            CursorIcon::Text
                        };

                    route.window.winit_window.set_cursor(cursor_icon);

                    // In case hyperlink range has cleaned trigger one more render
                    if route
                        .window
                        .screen
                        .context_manager
                        .current()
                        .has_hyperlink_range()
                    {
                        route
                            .window
                            .screen
                            .context_manager
                            .current_mut()
                            .set_hyperlink_range(None);
                        route.window.screen.context_manager.request_render();
                    }
                }

                route.window.screen.mouse.inside_text_area = inside_text_area;
                route.window.screen.mouse.square_side = square_side;

                if is_selecting {
                    route.window.screen.update_selection(point, square_side);
                    route.window.screen.context_manager.request_render();
                } else if square_changed
                    && route.window.screen.has_mouse_motion_and_drag()
                {
                    if lmb_pressed {
                        route.window.screen.mouse_report(32, ElementState::Pressed);
                    } else if route.window.screen.mouse.middle_button_state
                        == ElementState::Pressed
                    {
                        route.window.screen.mouse_report(33, ElementState::Pressed);
                    } else if route.window.screen.mouse.right_button_state
                        == ElementState::Pressed
                    {
                        route.window.screen.mouse_report(34, ElementState::Pressed);
                    } else if route.window.screen.has_mouse_motion() {
                        route.window.screen.mouse_report(35, ElementState::Pressed);
                    }
                }
            }

            WindowEvent::MouseWheel { delta, phase, .. } => {
                if route.path != RoutePath::Terminal {
                    return;
                }

                if self.config.hide_cursor_when_typing {
                    route.window.winit_window.set_cursor_visible(true);
                }

                match delta {
                    MouseScrollDelta::LineDelta(columns, lines) => {
                        let current_id = route.window.screen.ctx().current().rich_text_id;
                        if let Some(layout) =
                            route.window.screen.sugarloaf.get_text_layout(&current_id)
                        {
                            let new_scroll_px_x = columns * layout.font_size;
                            let new_scroll_px_y = lines * layout.font_size;
                            route
                                .window
                                .screen
                                .scroll(new_scroll_px_x as f64, new_scroll_px_y as f64);
                        }
                    }
                    MouseScrollDelta::PixelDelta(mut lpos) => {
                        match phase {
                            TouchPhase::Started => {
                                // Reset offset to zero.
                                route.window.screen.mouse.accumulated_scroll =
                                    Default::default();
                            }
                            TouchPhase::Moved => {
                                // When the angle between (x, 0) and (x, y) is lower than ~25 degrees
                                // (cosine is larger that 0.9) we consider this scrolling as horizontal.
                                if lpos.x.abs() / lpos.x.hypot(lpos.y) > 0.9 {
                                    lpos.y = 0.;
                                } else {
                                    lpos.x = 0.;
                                }

                                route.window.screen.scroll(lpos.x, lpos.y);
                            }
                            _ => (),
                        }
                    }
                }

                route.request_redraw();
            }

            WindowEvent::KeyboardInput {
                is_synthetic: false,
                event: key_event,
                ..
            } => {
                if route.has_key_wait(&key_event, &mut self.router.clipboard) {
                    if route.path != RoutePath::Terminal
                        && key_event.state == ElementState::Released
                    {
                        // Scheduler must be cleaned after leave the terminal route
                        self.scheduler.unschedule(TimerId::new(
                            Topic::Render,
                            route.window.screen.ctx().current_route(),
                        ));
                    }
                    return;
                }

                route.window.screen.context_manager.set_last_typing();
                route
                    .window
                    .screen
                    .process_key_event(&key_event, &mut self.router.clipboard);

                if key_event.state == ElementState::Released
                    && self.config.hide_cursor_when_typing
                {
                    route.window.winit_window.set_cursor_visible(false);
                }
            }

            WindowEvent::Ime(ime) => {
                if route.window.screen.renderer.assistant.is_active() {
                    return;
                }

                match ime {
                    Ime::Commit(text) => {
                        // Don't use bracketed paste for single char input.
                        route.window.screen.paste(&text, text.chars().count() > 1);
                    }
                    Ime::Preedit(text, cursor_offset) => {
                        let preedit = if text.is_empty() {
                            None
                        } else {
                            Some(Preedit::new(text, cursor_offset.map(|offset| offset.0)))
                        };

                        if route.window.screen.context_manager.current().ime.preedit()
                            != preedit.as_ref()
                        {
                            route
                                .window
                                .screen
                                .context_manager
                                .current_mut()
                                .ime
                                .set_preedit(preedit);
                            route.request_redraw();
                        }
                    }
                    Ime::Enabled => {
                        route
                            .window
                            .screen
                            .context_manager
                            .current_mut()
                            .ime
                            .set_enabled(true);
                    }
                    Ime::Disabled => {
                        route
                            .window
                            .screen
                            .context_manager
                            .current_mut()
                            .ime
                            .set_enabled(false);
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                on_touch(route, touch, &mut self.router.clipboard);
            }

            WindowEvent::Focused(focused) => {
                if self.config.hide_cursor_when_typing {
                    route.window.winit_window.set_cursor_visible(true);
                }

                let has_regained_focus = !route.window.is_focused && focused;
                route.window.is_focused = focused;

                if has_regained_focus {
                    route.request_redraw();
                }

                route.window.screen.on_focus_change(focused);
            }

            WindowEvent::Occluded(occluded) => {
                let was_occluded = route.window.is_occluded;
                route.window.is_occluded = occluded;

                // If window was occluded and is now visible, mark for one-time render
                if was_occluded && !occluded {
                    route.window.needs_render_after_occlusion = true;
                }
            }

            WindowEvent::ThemeChanged(new_theme) => {
                if self.config.force_theme.is_some() {
                    return;
                }
                update_colors_based_on_theme(&mut self.config, Some(new_theme));
                route.window.screen.update_config(
                    &self.config,
                    &self.router.font_library,
                    false,
                );
                route.window.configure_window(&self.config);
            }

            WindowEvent::DroppedFile(path) => {
                if route.window.screen.renderer.assistant.is_active() {
                    return;
                }

                let path: String = path.to_string_lossy().into();
                route.window.screen.paste(&(path + " "), true);
            }

            WindowEvent::Resized(new_size) => {
                if new_size.width == 0 || new_size.height == 0 {
                    return;
                }

                route.window.screen.resize(new_size);
            }

            WindowEvent::ScaleFactorChanged {
                inner_size_writer: _,
                scale_factor,
            } => {
                let scale = scale_factor as f32;
                route
                    .window
                    .screen
                    .set_scale(scale, route.window.winit_window.inner_size());
                route.window.update_vblank_interval();
            }

            WindowEvent::RedrawRequested => {
                // let start = std::time::Instant::now();
                route.window.winit_window.pre_present_notify();

                route.begin_render();

                match route.path {
                    RoutePath::Welcome => {
                        route.window.screen.render_welcome();
                    }
                    RoutePath::Terminal | RoutePath::ConfirmQuit => {
                        if route.path == RoutePath::ConfirmQuit {
                            let dim = route.window.screen.ctx().current().dimension;
                            crate::router::routes::dialog::screen(
                                &mut route.window.screen.sugarloaf,
                                &dim,
                                "want to quit?",
                                "yes (y)",
                                "no (n)",
                            );
                        }

                        if let Some(window_update) = route.window.screen.render() {
                            use crate::context::renderable::{
                                BackgroundState, WindowUpdate,
                            };
                            match window_update {
                                WindowUpdate::Background(bg_state) => {
                                    // for now setting this as allowed because it fails on linux builds
                                    #[allow(unused_variables)]
                                    let bg_color = match bg_state {
                                        BackgroundState::Set(color) => color,
                                        BackgroundState::Reset => {
                                            self.config.colors.background.1
                                        }
                                    };

                                    #[cfg(target_os = "macos")]
                                    {
                                        route.window.winit_window.set_background_color(
                                            bg_color.r, bg_color.g, bg_color.b,
                                            bg_color.a,
                                        );
                                    }

                                    #[cfg(target_os = "windows")]
                                    {
                                        use rio_window::platform::windows::WindowExtWindows;
                                        route
                                            .window
                                            .winit_window
                                            .set_title_bar_background_color(
                                                bg_color.r, bg_color.g, bg_color.b,
                                                bg_color.a,
                                            );
                                    }
                                }
                            }
                        }

                        // Update IME cursor position after rendering to ensure it's current
                        route.window.screen.update_ime_cursor_position_if_needed(
                            &route.window.winit_window,
                        );
                    }
                }

                // let duration = start.elapsed();
                // println!("Time elapsed in render() is: {:?}", duration);
                // }

                let island_needs_redraw = route
                    .window
                    .screen
                    .renderer
                    .island
                    .as_ref()
                    .is_some_and(|i| i.needs_rename_redraw());
                if self.config.renderer.strategy.is_game()
                    || route.path == RoutePath::Welcome
                    || route.path == RoutePath::ConfirmQuit
                    || route.window.screen.renderer.command_palette.is_enabled()
                    || island_needs_redraw
                {
                    route.request_redraw();
                } else if route
                    .window
                    .screen
                    .ctx()
                    .current()
                    .renderable_content
                    .pending_update
                    .is_dirty()
                {
                    route.schedule_redraw(
                        &mut self.scheduler,
                        route.window.screen.ctx().current_route(),
                    );
                }

                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let control_flow = match self.scheduler.update() {
            Some(instant) => ControlFlow::WaitUntil(instant),
            None => ControlFlow::Wait,
        };
        event_loop.set_control_flow(control_flow);
    }

    fn open_config(&mut self, event_loop: &ActiveEventLoop) {
        if self.config.navigation.open_config_with_split {
            self.router.open_config_split(&self.config);
        } else {
            self.router.open_config_window(
                event_loop,
                self.event_proxy.clone(),
                &self.config,
            );
        }
    }

    fn hook_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        key: &rio_window::event::KeyEvent,
        modifiers: &rio_window::event::Modifiers,
    ) {
        let window_id = match self.router.get_focused_route() {
            Some(window_id) => window_id,
            None => return,
        };

        let route = match self.router.routes.get_mut(&window_id) {
            Some(window) => window,
            None => return,
        };

        // For menu-triggered events, we need to temporarily set the correct modifiers
        // since menu events don't trigger ModifiersChanged events.
        let original_modifiers = route.window.screen.modifiers;

        // Use the modifiers passed from the menu action
        route.window.screen.set_modifiers(*modifiers);

        // Process the key event
        route
            .window
            .screen
            .process_key_event(key, &mut self.router.clipboard);

        // Restore the original modifiers
        route.window.screen.set_modifiers(original_modifiers);
    }

    // Emitted when the event loop is being shut down.
    // This is irreversible - if this event is emitted, it is guaranteed to be the last event that gets emitted.
    // You generally want to treat this as an “do on quit” event.
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Ensure that all the windows are dropped, so the destructors for
        // Renderer and contexts ran.
        self.router.routes.clear();

        // SAFETY: The clipboard must be dropped before the event loop, so
        // replace it with a safe no-op placeholder.
        self.router.clipboard = Clipboard::new_nop();

        std::process::exit(0);
    }
}

#[cfg(all(
    feature = "audio",
    not(target_os = "macos"),
    not(target_os = "windows")
))]
fn play_bell_sound() -> Result<(), Box<dyn Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("No output device available")?;

    let config = device.default_output_config()?;

    match config.sample_format() {
        cpal::SampleFormat::F32 => run_bell::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run_bell::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => run_bell::<u16>(&device, &config.into()),
        _ => Err("Unsupported sample format".into()),
    }
}

#[cfg(all(
    feature = "audio",
    not(target_os = "macos"),
    not(target_os = "windows")
))]
fn run_bell<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
) -> Result<(), Box<dyn Error>>
where
    T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
{
    let sample_rate = config.sample_rate.0 as f32;
    let channels = config.channels as usize;
    let duration_secs = crate::constants::BELL_DURATION.as_secs_f32();
    let total_samples = (sample_rate * duration_secs) as usize;

    let mut sample_clock = 0f32;
    let mut samples_played = 0usize;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            for frame in data.chunks_mut(channels) {
                if samples_played >= total_samples {
                    for sample in frame.iter_mut() {
                        *sample = T::from_sample(0.0);
                    }
                } else {
                    let value = (sample_clock * 440.0 * 2.0 * std::f32::consts::PI
                        / sample_rate)
                        .sin()
                        * 0.2;
                    for sample in frame.iter_mut() {
                        *sample = T::from_sample(value);
                    }
                    sample_clock += 1.0;
                    samples_played += 1;
                }
            }
        },
        |err| tracing::error!("Audio stream error: {}", err),
        None,
    )?;

    stream.play()?;
    std::thread::sleep(crate::constants::BELL_DURATION);

    Ok(())
}
