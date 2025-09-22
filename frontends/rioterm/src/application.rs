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
use rio_backend::config::colors::ColorRgb;
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
}

impl Application<'_> {
    pub fn new<'app>(
        config: rio_backend::config::Config,
        config_error: Option<rio_backend::config::ConfigError>,
        event_loop: &EventLoop<EventPayload>,
    ) -> Application<'app> {
        // SAFETY: Since this takes a pointer to the winit event loop, it MUST be dropped first,
        // which is done in `loop_exiting`.
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

        #[cfg(target_os = "macos")]
        event_loop.set_confirm_before_quit(config.confirm_before_quit);

        Application {
            config,
            event_proxy,
            router,
            scheduler,
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

    fn handle_visual_bell(&mut self, window_id: WindowId) {
        if let Some(route) = self.router.routes.get_mut(&window_id) {
            route.window.screen.renderer.trigger_visual_bell();

            // Mark content as dirty to ensure render happens
            route
                .window
                .screen
                .ctx_mut()
                .current_mut()
                .renderable_content
                .pending_update
                .set_dirty();

            // Force immediate render to show the bell
            route.request_redraw();

            // Schedule a render after the bell duration to clear it
            let timer_id =
                TimerId::new(Topic::Render, route.window.screen.ctx().current_route());
            let event = EventPayload::new(RioEventType::Rio(RioEvent::Render), window_id);

            // Schedule render to clear bell effect after visual bell duration
            self.scheduler.schedule(
                event,
                crate::constants::BELL_DURATION,
                false,
                timer_id,
            );
        }
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

        update_colors_based_on_theme(&mut self.config, event_loop.system_theme());

        self.router.create_window(
            event_loop,
            self.event_proxy.clone(),
            &self.config,
            None,
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
                            route.window.screen.ctx_mut().get_mut(route_id)
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

            RioEventType::Rio(RioEvent::Wakeup(route_id)) => {
                if self.config.renderer.strategy.is_event_based() {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        // Skip rendering for unfocused windows if configured
                        if self.config.renderer.disable_unfocused_render
                            && !route.window.is_focused
                        {
                            tracing::trace!("Wakeup: Skipping unfocused window");
                            return;
                        }

                        // Skip rendering for occluded windows if configured
                        if self.config.renderer.disable_occluded_render
                            && route.window.is_occluded
                            && !route.window.needs_render_after_occlusion
                        {
                            tracing::trace!("Wakeup: Skipping occluded window");
                            return;
                        }

                        tracing::trace!(
                            "Wakeup: Marking route {} for damage check",
                            route_id
                        );

                        // Mark the renderable content as needing to check for damage
                        // The actual damage retrieval will happen during render
                        if let Some(ctx_item) =
                            route.window.screen.ctx_mut().get_mut(route_id)
                        {
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

                    for graphic_data in queues.pending {
                        sugarloaf.graphics.insert(graphic_data);
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
                    self.router.font_library = Box::new(new_font_library.0);
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
                        update_colors_based_on_theme(&mut self.config, system_theme);
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
                    if cfg!(target_os = "macos") && self.config.confirm_before_quit {
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
                        .should_close_context_manager(route_id)
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

                        // Set UI damage for cursor line
                        route
                            .window
                            .screen
                            .ctx_mut()
                            .current_mut()
                            .renderable_content
                            .pending_update
                            .set_ui_damage(rio_backend::event::TerminalDamage::Partial(
                                [rio_backend::crosswords::LineDamage::new(
                                    cursor_line,
                                    true,
                                )]
                                .into_iter()
                                .collect(),
                            ));

                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::Bell) => {
                // Handle visual bell
                if self.config.bell.visual {
                    self.handle_visual_bell(window_id);
                }

                // Handle audio bell
                if self.config.bell.audio {
                    self.handle_audio_bell();
                }
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
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route.window.is_focused {
                        let text = format(
                            self.router
                                .clipboard
                                .borrow_mut()
                                .get(clipboard_type)
                                .as_str(),
                        );
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
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route.window.is_focused {
                        self.router
                            .clipboard
                            .borrow_mut()
                            .set(clipboard_type, content);
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
                // MacOS doesn't exit the loop
                if cfg!(target_os = "macos") && self.config.confirm_before_quit {
                    self.router.routes.remove(&window_id);
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

                #[cfg(target_os = "macos")]
                {
                    if route.window.is_macos_deadzone {
                        return;
                    }
                }

                match state {
                    ElementState::Pressed => {
                        // In case need to switch grid current
                        route.window.screen.select_current_based_on_mouse();

                        if route.window.screen.trigger_hyperlink() {
                            return;
                        }

                        // Process mouse press before bindings to update the `click_state`.
                        if !route.window.screen.modifiers.state().shift_key()
                            && route.window.screen.mouse_mode()
                        {
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

                            route.window.screen.process_mouse_bindings(button);
                        } else {
                            // Calculate time since the last click to handle double/triple clicks.
                            let now = Instant::now();
                            let elapsed =
                                now - route.window.screen.mouse.last_click_timestamp;
                            route.window.screen.mouse.last_click_timestamp = now;

                            let threshold = Duration::from_millis(300);
                            let mouse = &route.window.screen.mouse;
                            route.window.screen.mouse.click_state = match mouse
                                .click_state
                            {
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

                            // Load mouse point, treating message bar and padding as the closest square.
                            let display_offset = route.window.screen.display_offset();

                            if let MouseButton::Left = button {
                                let pos =
                                    route.window.screen.mouse_position(display_offset);
                                route.window.screen.on_left_click(pos);
                            }

                            route.request_redraw();
                        }
                        route.window.screen.process_mouse_bindings(button);
                    }
                    ElementState::Released => {
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
                            && route.window.screen.trigger_hint()
                        {
                            return;
                        }

                        if let MouseButton::Left | MouseButton::Right = button {
                            // Copy selection on release, to prevent flooding the display server.
                            route.window.screen.copy_selection(ClipboardType::Selection);
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

                let lmb_pressed =
                    route.window.screen.mouse.left_button_state == ElementState::Pressed;
                let rmb_pressed =
                    route.window.screen.mouse.right_button_state == ElementState::Pressed;

                let has_selection = !route.window.screen.selection_is_empty();

                #[cfg(target_os = "macos")]
                {
                    // Dead zone for MacOS only
                    // e.g: Dragging the terminal
                    if !has_selection
                        && !route.window.screen.context_manager.config.is_native
                        && route.window.screen.is_macos_deadzone(y)
                    {
                        route.window.winit_window.set_cursor(CursorIcon::Default);

                        route.window.is_macos_deadzone = true;
                        return;
                    }

                    route.window.is_macos_deadzone = false;
                }

                if has_selection && (lmb_pressed || rmb_pressed) {
                    route.window.screen.update_selection_scrolling(y);
                }

                let display_offset = route.window.screen.display_offset();
                let old_point = route.window.screen.mouse_position(display_offset);

                let layout = route.window.screen.sugarloaf.window_size();

                let x = x.clamp(0.0, (layout.width as i32 - 1).into()) as usize;
                let y = y.clamp(0.0, (layout.height as i32 - 1).into()) as usize;
                route.window.screen.mouse.x = x;
                route.window.screen.mouse.y = y;

                let point = route.window.screen.mouse_position(display_offset);

                let square_changed = old_point != point;

                let inside_text_area = route.window.screen.contains_point(x, y);
                let square_side = route.window.screen.side_by_pos(x);

                // If the mouse hasn't changed cells, do nothing.
                if !square_changed
                    && route.window.screen.mouse.square_side == square_side
                    && route.window.screen.mouse.inside_text_area == inside_text_area
                {
                    return;
                }

                if route.window.screen.update_highlighted_hints() {
                    route.window.winit_window.set_cursor(CursorIcon::Pointer);
                    route.window.screen.context_manager.request_render();
                } else {
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

                if (lmb_pressed || rmb_pressed)
                    && (route.window.screen.modifiers.state().shift_key()
                        || !route.window.screen.mouse_mode())
                {
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
                        let layout = route.window.screen.sugarloaf.rich_text_layout(&0);
                        let new_scroll_px_x = columns * layout.font_size;
                        let new_scroll_px_y = lines * layout.font_size;
                        route
                            .window
                            .screen
                            .scroll(new_scroll_px_x as f64, new_scroll_px_y as f64);
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
            }

            WindowEvent::KeyboardInput {
                is_synthetic: false,
                event: key_event,
                ..
            } => {
                if route.has_key_wait(&key_event) {
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
                route.window.screen.process_key_event(&key_event);

                if key_event.state == ElementState::Released
                    && self.config.hide_cursor_when_typing
                {
                    route.window.winit_window.set_cursor_visible(false);
                }
            }

            WindowEvent::Ime(ime) => {
                if route.path == RoutePath::Assistant {
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
                on_touch(route, touch);
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
                update_colors_based_on_theme(&mut self.config, Some(new_theme));
                route.window.screen.update_config(
                    &self.config,
                    &self.router.font_library,
                    false,
                );
                route.window.configure_window(&self.config);
            }

            WindowEvent::DroppedFile(path) => {
                if route.path == RoutePath::Assistant {
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
                    RoutePath::Assistant => {
                        route.window.screen.render_assistant(&route.assistant);
                    }
                    RoutePath::Welcome => {
                        route.window.screen.render_welcome();
                    }
                    RoutePath::Terminal => {
                        route.window.screen.render();
                        // Update IME cursor position after rendering to ensure it's current
                        route.window.screen.update_ime_cursor_position_if_needed(
                            &route.window.winit_window,
                        );
                    }
                    RoutePath::ConfirmQuit => {
                        route.window.screen.render_dialog(
                            "Quit Rio?",
                            "Continue -> press escape key",
                            "Quit -> press enter key",
                        );
                    }
                }

                // let duration = start.elapsed();
                // println!("Time elapsed in render() is: {:?}", duration);
                // }

                if self.config.renderer.strategy.is_game() {
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
        route.window.screen.process_key_event(key);

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

        // SAFETY: The clipboard must be dropped before the event loop, so use the nop clipboard
        // as a safe placeholder.
        self.router.clipboard =
            std::rc::Rc::new(std::cell::RefCell::new(Clipboard::new_nop()));

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
