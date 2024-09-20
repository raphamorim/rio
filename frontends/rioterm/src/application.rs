use crate::event::{ClickState, EventPayload, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::renderer::utils::update_colors_based_on_theme;
use crate::router::{routes::RoutePath, Router};
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::touch::on_touch;
use crate::watcher::configuration_file_updates;
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

    pub fn run(
        &mut self,
        event_loop: EventLoop<EventPayload>,
    ) -> Result<(), Box<dyn Error>> {
        let result = event_loop.run_app(self);
        result.map_err(Into::into)
    }
}

impl ApplicationHandler<EventPayload> for Application<'_> {
    fn resumed(&mut self, _active_event_loop: &ActiveEventLoop) {
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            // This is a hacky solution to force an update to the window on linux
            // Fix is only for windows with opacity that aren't being computed at all
            if self.config.window.opacity < 1. || self.config.window.blur {
                for (_id, route) in self.router.routes.iter_mut() {
                    route.update_config(&self.config, &self.router.font_library);

                    route.request_redraw();
                }
            }
        }
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        if cause != StartCause::Init && cause != StartCause::CreateWindow {
            return;
        }

        update_colors_based_on_theme(&mut self.config, event_loop.system_theme());

        self.router.create_window(
            event_loop,
            self.event_proxy.clone(),
            &self.config,
            None,
        );

        tracing::info!("Initialisation complete");
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: EventPayload) {
        let window_id = event.window_id;
        match event.payload {
            RioEventType::Rio(RioEvent::Render) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if self.config.renderer.disable_unfocused_render
                        && !route.window.is_focused
                    {
                        return;
                    }

                    route.request_redraw();
                }
            }
            RioEventType::Rio(RioEvent::RenderRoute(route_id)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if self.config.renderer.disable_unfocused_render
                        && !route.window.is_focused
                    {
                        return;
                    }

                    if route_id == route.window.screen.ctx().current_route() {
                        if self.config.renderer.max_fps == 0 {
                            route.request_redraw();
                        } else {
                            let timer_id = TimerId::new(Topic::RenderRoute, window_id);
                            let event = EventPayload::new(
                                RioEventType::Rio(RioEvent::Render),
                                window_id,
                            );

                            if !self.scheduler.scheduled(timer_id) {
                                self.scheduler.schedule(
                                    event,
                                    Duration::from_millis(
                                        1000 / self.config.renderer.max_fps,
                                    ),
                                    false,
                                    timer_id,
                                );
                            }
                        }
                    }
                }
            }
            RioEventType::Rio(RioEvent::UpdateGraphicLibrary) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let mut terminal =
                        route.window.screen.ctx().current().terminal.lock();
                    let graphics = terminal.graphics_take_queues();
                    drop(terminal);
                    if let Some(graphic_queues) = graphics {
                        for graphic_data in graphic_queues.pending {
                            route.window.screen.sugarloaf.graphics.insert(graphic_data);
                        }

                        for graphic_data in graphic_queues.remove_queue {
                            route.window.screen.sugarloaf.graphics.remove(&graphic_data);
                        }
                    }
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
                for (_id, route) in self.router.routes.iter_mut() {
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

                    route.update_config(&self.config, &self.router.font_library);
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
                        .should_close_context_manager(route_id)
                    {
                        self.router.routes.remove(&window_id);

                        // Unschedule pending events.
                        self.scheduler.unschedule_window(window_id);

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
                    if route.window.has_frame {
                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::CursorBlinkingChangeOnRoute(route_id)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route_id == route.window.screen.ctx().current_route() {
                        route.request_redraw();
                    }
                }
            }
            RioEventType::Rio(RioEvent::PrepareRender(millis)) => {
                let timer_id = TimerId::new(Topic::Render, window_id);
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
            RioEventType::Rio(RioEvent::PrepareRenderOnRoute(millis, route_id)) => {
                let timer_id = TimerId::new(Topic::RenderRoute, window_id);
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
                let timer_id = TimerId::new(Topic::CursorBlinking, window_id);
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
            RioEventType::Rio(RioEvent::MouseCursorDirty) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    route.window.screen.reset_mouse();
                }
            }
            RioEventType::Rio(RioEvent::Scroll(scroll)) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let mut terminal =
                        route.window.screen.ctx().current().terminal.lock();
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
                    let layout = route.window.screen.sugarloaf.layout();
                    let text =
                        format(crate::renderer::utils::terminal_dimensions(&layout));
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
                // TODO: colors could be coming terminal as well
                // if colors has been declaratively changed
                // Rio doesn't cover this case yet.
                //
                // In the future should try first get
                // from Crosswords then state colors
                // screen.colors()[index] or screen.renderer.colors[index]
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    let color = route.window.screen.renderer.colors[index];
                    let rgb = ColorRgb::from_color_arr(color);
                    route
                        .window
                        .screen
                        .ctx_mut()
                        .current_mut()
                        .messenger
                        .send_bytes(format(rgb).into_bytes());
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
                    //
                    // TODO: Reimplement this flow
                    let mut should_revert_to_previous_config: Option<
                        rio_backend::config::Config,
                    > = None;
                    if working_dir_overwrite.is_some() {
                        let current_config = &self.config;
                        should_revert_to_previous_config = Some(current_config.clone());

                        let config = rio_backend::config::Config {
                            working_dir: working_dir_overwrite,
                            ..current_config.clone()
                        };
                        self.config = config;
                    }

                    self.router.create_native_tab(
                        event_loop,
                        self.event_proxy.clone(),
                        &self.config,
                        Some(&route.window.winit_window.tabbing_identifier()),
                        None,
                    );

                    if let Some(old_config) = should_revert_to_previous_config {
                        self.config = old_config;
                    }
                }
            }
            RioEventType::Rio(RioEvent::CreateConfigEditor) => {
                self.router.open_config_window(
                    event_loop,
                    self.event_proxy.clone(),
                    &self.config,
                );
            }
            #[cfg(target_os = "macos")]
            RioEventType::Rio(RioEvent::CloseWindow) => {
                if let Some(route) = self.router.routes.get_mut(&window_id) {
                    if route.window.winit_window.num_tabs() > 1 {
                        self.router.routes.remove(&window_id);
                    } else {
                        // In MacOS: Close last tab will work
                        // leading to hide and run Rio in background
                        let routes_len = self.router.routes.len();
                        self.router.routes.remove(&window_id);
                        // If was the last last window opened then hide the application
                        if routes_len == 1 {
                            let config = &self.config;
                            event_loop.hide_application();
                            self.router.create_window(
                                event_loop,
                                self.event_proxy.clone(),
                                config,
                                None,
                            );
                        }
                    }
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
                self.router.routes.remove(&window_id);

                if self.router.routes.is_empty() {
                    event_loop.exit();
                }
            }

            WindowEvent::ModifiersChanged(modifiers) => {
                route.window.screen.set_modifiers(modifiers);

                if route.window.screen.search_nearest_hyperlink_from_pos() {
                    if self.config.hide_cursor_when_typing {
                        route.window.winit_window.set_cursor_visible(true);
                    }

                    route.window.winit_window.set_cursor(CursorIcon::Pointer);
                    route.window.screen.context_manager.schedule_render(60);
                }
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

                let layout = route.window.screen.sugarloaf.layout();

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

                if route.window.screen.search_nearest_hyperlink_from_pos() {
                    route.window.winit_window.set_cursor(CursorIcon::Pointer);
                    route.window.screen.context_manager.schedule_render(60);
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
                    if route.window.screen.renderer.has_hyperlink_range() {
                        route.window.screen.renderer.set_hyperlink_range(None);
                        route.window.screen.context_manager.schedule_render(60);
                    }
                }

                route.window.screen.mouse.inside_text_area = inside_text_area;
                route.window.screen.mouse.square_side = square_side;

                if (lmb_pressed || rmb_pressed)
                    && (route.window.screen.modifiers.state().shift_key()
                        || !route.window.screen.mouse_mode())
                {
                    route.window.screen.update_selection(point, square_side);
                    route.window.screen.context_manager.schedule_render(60);
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
                        let layout = route.window.screen.sugarloaf.layout();
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
                        self.scheduler
                            .unschedule(TimerId::new(Topic::Render, window_id));
                    }
                    return;
                }

                route.window.screen.renderer.last_typing = Some(Instant::now());
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

                        if route.window.screen.ime.preedit() != preedit.as_ref() {
                            route.window.screen.ime.set_preedit(preedit);
                            if route.window.has_frame {
                                route.request_redraw();
                            }
                        }
                    }
                    Ime::Enabled => {
                        route.window.screen.ime.set_enabled(true);
                    }
                    Ime::Disabled => {
                        route.window.screen.ime.set_enabled(false);
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
                route.window.is_occluded = occluded;
            }

            WindowEvent::ThemeChanged(new_theme) => {
                update_colors_based_on_theme(&mut self.config, Some(new_theme));
                route
                    .window
                    .screen
                    .update_config(&self.config, &self.router.font_library);
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
            }

            WindowEvent::RedrawRequested => {
                // let start = std::time::Instant::now();
                route.window.has_updates = false;

                route.window.winit_window.pre_present_notify();
                match route.path {
                    RoutePath::Assistant => {
                        route.window.screen.render_assistant(&route.assistant);
                    }
                    RoutePath::Welcome => {
                        route.window.screen.render_welcome();
                    }
                    RoutePath::Terminal => {
                        route.window.screen.render();
                    }
                    RoutePath::ConfirmQuit => {
                        route
                            .window
                            .screen
                            .render_dialog("Do you want to leave Rio?");
                    }
                }
                // let duration = start.elapsed();
                // println!("Time elapsed in render() is: {:?}", duration);
                // }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let control_flow = match self.scheduler.update() {
            Some(instant) => ControlFlow::WaitUntil(instant),
            None => {
                self.router.update_titles();
                ControlFlow::Wait
            }
        };
        event_loop.set_control_flow(control_flow);
    }

    fn open_config(&mut self, event_loop: &ActiveEventLoop) {
        self.router.open_config_window(
            event_loop,
            self.event_proxy.clone(),
            &self.config,
        );
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
        std::mem::swap(
            &mut self.router.clipboard,
            &mut std::rc::Rc::new(std::cell::RefCell::new(Clipboard::new_nop())),
        );

        std::process::exit(0);
    }
}
