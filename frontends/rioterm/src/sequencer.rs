use crate::event::{ClickState, EventPayload, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::router::{RouteWindow, Router};
use crate::routes::RoutePath;
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::touch::on_touch;
use crate::watcher::configuration_file_updates;
use rio_backend::clipboard::ClipboardType;
use rio_backend::config::colors::ColorRgb;
use std::error::Error;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event::{
    ElementState, Event, Ime, MouseButton, MouseScrollDelta, StartCause, TouchPhase,
    WindowEvent,
};
use winit::event_loop::ControlFlow;
use winit::event_loop::{DeviceEvents, EventLoop};
#[cfg(target_os = "macos")]
use winit::platform::macos::ActiveEventLoopExtMacOS;
#[cfg(target_os = "macos")]
use winit::platform::macos::WindowExtMacOS;
use winit::platform::run_on_demand::EventLoopExtRunOnDemand;
use winit::window::{CursorIcon, Fullscreen};

pub struct Sequencer {
    config: Rc<rio_backend::config::Config>,
    event_proxy: Option<EventProxy>,
    router: Router,
}

impl Sequencer {
    pub fn new(
        config: rio_backend::config::Config,
        config_error: Option<rio_backend::config::ConfigError>,
    ) -> Sequencer {
        let mut router = Router::new();
        if let Some(error) = config_error {
            router.propagate_error_to_next_route(error.into());
        }

        Sequencer {
            config: Rc::new(config),
            event_proxy: None,
            router,
        }
    }

    pub async fn run(
        &mut self,
        mut event_loop: EventLoop<EventPayload>,
    ) -> Result<(), Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        self.event_proxy = Some(EventProxy::new(proxy.clone()));
        let _ = configuration_file_updates(
            rio_backend::config::config_dir_path(),
            self.event_proxy.clone().unwrap(),
        );
        let mut scheduler = Scheduler::new(proxy);

        let mut window =
            RouteWindow::new(&event_loop, &self.config, &self.router.font_library, None)
                .await?;
        window.is_focused = true;
        self.router.create_route_from_window(window);

        event_loop.listen_device_events(DeviceEvents::Never);
        #[allow(deprecated)]
        let _ = event_loop.run_on_demand(move |event, event_loop_window_target| {
            match event {
                Event::UserEvent(EventPayload {
                    payload, window_id, ..
                }) => {
                    match payload {
                        RioEventType::Rio(RioEvent::Wakeup) => {
                            // Emitted when the application has been resumed.
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                route.redraw();
                            }
                        }
                        RioEventType::Rio(RioEvent::Render) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                if self.config.renderer.disable_unfocused_render
                                    && !route.window.is_focused
                                {
                                    return;
                                }
                                route.redraw();
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
                                        route
                                            .window
                                            .screen
                                            .sugarloaf
                                            .add_graphic(graphic_data);
                                    }

                                    for graphic_data in graphic_queues.remove_queue {
                                        route
                                            .window
                                            .screen
                                            .sugarloaf
                                            .remove_graphic(&graphic_data);
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
                            let (config, config_error) =
                                match rio_backend::config::Config::try_load() {
                                    Ok(config) => (config, None),
                                    Err(error) => (
                                        rio_backend::config::Config::default(),
                                        Some(error),
                                    ),
                                };

                            self.config = config.into();
                            for (_id, route) in self.router.routes.iter_mut() {
                                route.update_config(
                                    &self.config,
                                    &self.router.font_library,
                                );

                                if let Some(error) = &config_error {
                                    route.report_error(&error.to_owned().into());
                                } else {
                                    route.clear_errors();
                                }

                                route.redraw();
                            }
                        }
                        RioEventType::Rio(RioEvent::Exit) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                if self.config.confirm_before_quit {
                                    route.confirm_quit();
                                    route.redraw();
                                } else {
                                    route.quit();
                                }
                            }
                        }
                        RioEventType::Rio(RioEvent::CloseTerminal) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                if !route.try_close_existent_tab() {
                                    self.router.routes.remove(&window_id);

                                    if self.router.routes.is_empty() {
                                        event_loop_window_target.exit();
                                    }
                                }
                            }
                        }
                        RioEventType::Rio(RioEvent::CursorBlinkingChange) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                route.window.winit_window.request_redraw();
                            }
                        }
                        RioEventType::Rio(RioEvent::PrepareRender(millis)) => {
                            let timer_id = TimerId::new(Topic::Render, 0);
                            let event = EventPayload::new(
                                RioEventType::Rio(RioEvent::Render),
                                window_id,
                            );

                            if !scheduler.scheduled(timer_id) {
                                scheduler.schedule(
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
                        RioEventType::Rio(RioEvent::TitleWithSubtitle(
                            title,
                            subtitle,
                        )) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                route.set_window_title(&title);
                                route.set_window_subtitle(&subtitle);
                            }
                        }
                        RioEventType::BlinkCursor | RioEventType::BlinkCursorTimeout => {}
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
                        RioEventType::Rio(RioEvent::ClipboardLoad(
                            clipboard_type,
                            format,
                        )) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                if route.window.is_focused {
                                    let text = format(
                                        route
                                            .window
                                            .screen
                                            .clipboard_get(clipboard_type)
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
                        RioEventType::Rio(RioEvent::ClipboardStore(
                            clipboard_type,
                            content,
                        )) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                if route.window.is_focused {
                                    route
                                        .window
                                        .screen
                                        .clipboard_store(clipboard_type, content);
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
                        RioEventType::Rio(RioEvent::ColorRequest(index, format)) => {
                            // TODO: colors could be coming terminal as well
                            // if colors has been declaratively changed
                            // Rio doesn't cover this case yet.
                            //
                            // In the future should try first get
                            // from Crosrouteords then state colors
                            // screen.colors()[index] or screen.state.colors[index]
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                let color = route.window.screen.state.colors[index];
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
                                event_loop_window_target,
                                self.event_proxy.clone().unwrap(),
                                &self.config,
                                None,
                            );
                        }
                        #[cfg(target_os = "macos")]
                        RioEventType::Rio(RioEvent::CreateNativeTab(
                            working_dir_overwrite,
                        )) => {
                            if let Some(route) = self.router.routes.get(&window_id) {
                                route.redraw();

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
                                    let current_config = (*self.config).clone();
                                    should_revert_to_previous_config =
                                        Some(current_config.clone());

                                    let config = rio_backend::config::Config {
                                        working_dir: working_dir_overwrite,
                                        ..current_config
                                    };
                                    self.config = config.into();
                                }

                                self.router.create_native_tab(
                                    event_loop_window_target,
                                    self.event_proxy.clone().unwrap(),
                                    &self.config,
                                    Some(route.window.winit_window.tabbing_identifier()),
                                    None,
                                );

                                if let Some(old_config) = should_revert_to_previous_config
                                {
                                    self.config = old_config.into();
                                }
                            }
                        }
                        RioEventType::Rio(RioEvent::CreateConfigEditor) => {
                            self.router.open_config_window(
                                event_loop_window_target,
                                self.event_proxy.clone().unwrap(),
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
                                        event_loop_window_target.hide_application();
                                        self.router.create_window(
                                            event_loop_window_target,
                                            self.event_proxy.clone().unwrap(),
                                            &self.config,
                                            None,
                                        );
                                    }
                                }
                            }
                        }
                        #[cfg(target_os = "macos")]
                        RioEventType::Rio(RioEvent::SelectNativeTabByIndex(
                            tab_index,
                        )) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                route.window.winit_window.select_tab_at_index(tab_index);
                            }
                        }
                        #[cfg(target_os = "macos")]
                        RioEventType::Rio(RioEvent::SelectNativeTabLast) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                route.window.winit_window.select_tab_at_index(
                                    route.window.winit_window.num_tabs() - 1,
                                );
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
                            event_loop_window_target.hide_application();
                        }
                        #[cfg(target_os = "macos")]
                        RioEventType::Rio(RioEvent::HideOtherApplications) => {
                            event_loop_window_target.hide_other_applications();
                        }
                        RioEventType::Rio(RioEvent::Minimize(set_minimize)) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                route.window.winit_window.set_minimized(set_minimize);
                            }
                        }
                        RioEventType::Rio(RioEvent::ToggleFullScreen) => {
                            if let Some(route) = self.router.routes.get_mut(&window_id) {
                                match route.window.winit_window.fullscreen() {
                                    None => route.window.winit_window.set_fullscreen(
                                        Some(Fullscreen::Borderless(None)),
                                    ),
                                    _ => route.window.winit_window.set_fullscreen(None),
                                }
                            }
                        }
                        _ => {}
                    }
                }

                Event::NewEvents(StartCause::Init) => {
                    // noop
                }

                #[cfg(target_os = "macos")]
                Event::Opened { urls } => {
                    if !self.config.navigation.is_native() {
                        for url in urls {
                            self.router.create_window(
                                event_loop_window_target,
                                self.event_proxy.clone().unwrap(),
                                &self.config,
                                Some(&url),
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
                        for url in urls {
                            self.router.create_native_tab(
                                event_loop_window_target,
                                self.event_proxy.clone().unwrap(),
                                &self.config,
                                tab_id.clone(),
                                Some(&url),
                            );
                        }
                    }
                }

                Event::Resumed => {
                    #[cfg(not(any(target_os = "macos", windows)))]
                    {
                        // This is a hacky solution to force an update to the window on linux
                        // Fix is only for windows with opacity that aren't being computed at all
                        if self.config.window.background_opacity < 1.
                            || self.config.window.blur
                        {
                            for (_id, route) in self.router.routes.iter_mut() {
                                route.update_config(
                                    &self.config,
                                    &self.router.font_library,
                                );

                                route.redraw();
                            }
                        }
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    window_id,
                    ..
                } => {
                    self.router.routes.remove(&window_id);

                    if self.router.routes.is_empty() {
                        event_loop_window_target.exit();
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        route.window.screen.set_modifiers(modifiers);

                        if route.window.screen.search_nearest_hyperlink_from_pos() {
                            if self.config.hide_cursor_when_typing {
                                route.window.winit_window.set_cursor_visible(true);
                            }

                            route.window.winit_window.set_cursor(CursorIcon::Pointer);
                            route.window.screen.context_manager.schedule_render(60);
                        }
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::MouseInput { state, button, .. },
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
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
                                    route.window.screen.mouse.click_state =
                                        ClickState::None;

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
                                    let elapsed = now
                                        - route.window.screen.mouse.last_click_timestamp;
                                    route.window.screen.mouse.last_click_timestamp = now;

                                    let threshold = Duration::from_millis(300);
                                    let mouse = &route.window.screen.mouse;
                                    route.window.screen.mouse.click_state = match mouse
                                        .click_state
                                    {
                                        // Reset click state if button has changed.
                                        _ if button != mouse.last_click_button => {
                                            route.window.screen.mouse.last_click_button =
                                                button;
                                            ClickState::Click
                                        }
                                        ClickState::Click if elapsed < threshold => {
                                            ClickState::DoubleClick
                                        }
                                        ClickState::DoubleClick
                                            if elapsed < threshold =>
                                        {
                                            ClickState::TripleClick
                                        }
                                        _ => ClickState::Click,
                                    };

                                    // Load mouse point, treating message bar and padding as the closest square.
                                    let display_offset =
                                        route.window.screen.display_offset();

                                    if let MouseButton::Left = button {
                                        let pos = route
                                            .window
                                            .screen
                                            .mouse_position(display_offset);
                                        route.window.screen.on_left_click(pos);
                                    }

                                    route.window.winit_window.request_redraw();
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
                                    route
                                        .window
                                        .screen
                                        .copy_selection(ClipboardType::Selection);
                                }
                            }
                        }
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::CursorMoved { position, .. },
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if self.config.hide_cursor_when_typing {
                            route.window.winit_window.set_cursor_visible(true);
                        }

                        if route.path != RoutePath::Terminal {
                            route.window.winit_window.set_cursor(CursorIcon::Default);
                            return;
                        }

                        let x = position.x;
                        let y = position.y;

                        let lmb_pressed = route.window.screen.mouse.left_button_state
                            == ElementState::Pressed;
                        let rmb_pressed = route.window.screen.mouse.right_button_state
                            == ElementState::Pressed;

                        let has_selection = !route.window.screen.selection_is_empty();

                        #[cfg(target_os = "macos")]
                        {
                            // Dead zone for MacOS only
                            // e.g: Dragging the terminal
                            if !has_selection
                                && !route.window.screen.context_manager.config.is_native
                                && route.window.screen.is_macos_deadzone(y)
                            {
                                if route.window.screen.is_macos_deadzone_draggable(x) {
                                    if lmb_pressed || rmb_pressed {
                                        route.window.screen.clear_selection();
                                        route
                                            .window
                                            .winit_window
                                            .set_cursor(CursorIcon::Grabbing);
                                    } else {
                                        route
                                            .window
                                            .winit_window
                                            .set_cursor(CursorIcon::Grab);
                                    }
                                } else {
                                    route
                                        .window
                                        .winit_window
                                        .set_cursor(CursorIcon::Default);
                                }

                                route.window.is_macos_deadzone = true;
                                return;
                            }

                            route.window.is_macos_deadzone = false;
                        }

                        if has_selection && (lmb_pressed || rmb_pressed) {
                            route.window.screen.update_selection_scrolling(y);
                        }

                        let display_offset = route.window.screen.display_offset();
                        let old_point =
                            route.window.screen.mouse_position(display_offset);

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
                            && route.window.screen.mouse.inside_text_area
                                == inside_text_area
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
                            if route.window.screen.state.has_hyperlink_range() {
                                route.window.screen.state.set_hyperlink_range(None);
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
                                route
                                    .window
                                    .screen
                                    .mouse_report(32, ElementState::Pressed);
                            } else if route.window.screen.mouse.middle_button_state
                                == ElementState::Pressed
                            {
                                route
                                    .window
                                    .screen
                                    .mouse_report(33, ElementState::Pressed);
                            } else if route.window.screen.mouse.right_button_state
                                == ElementState::Pressed
                            {
                                route
                                    .window
                                    .screen
                                    .mouse_report(34, ElementState::Pressed);
                            } else if route.window.screen.has_mouse_motion() {
                                route
                                    .window
                                    .screen
                                    .mouse_report(35, ElementState::Pressed);
                            }
                        }
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::MouseWheel { delta, phase, .. },
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
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
                                route.window.screen.scroll(
                                    new_scroll_px_x as f64,
                                    new_scroll_px_y as f64,
                                );
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
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::KeyboardInput {
                            is_synthetic: false,
                            event: key_event,
                            ..
                        },
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if route.has_key_wait(&key_event) {
                            if route.path != RoutePath::Terminal
                                && key_event.state == ElementState::Released
                            {
                                // Scheduler must be cleaned after leave the terminal route
                                scheduler.unschedule(TimerId::new(Topic::Render, 0));
                                route.window.winit_window.request_redraw();
                            }
                            return;
                        }

                        route.window.screen.state.last_typing = Some(Instant::now());
                        route.window.screen.process_key_event(&key_event);

                        if key_event.state == ElementState::Released
                            && self.config.hide_cursor_when_typing
                        {
                            route.window.winit_window.set_cursor_visible(false);

                            // route.redraw();
                        }
                    }
                }

                Event::WindowEvent {
                    event: WindowEvent::Ime(ime),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if route.path == RoutePath::Assistant {
                            return;
                        }

                        match ime {
                            Ime::Commit(text) => {
                                // Don't use bracketed paste for single char input.
                                route
                                    .window
                                    .screen
                                    .paste(&text, text.chars().count() > 1);
                            }
                            Ime::Preedit(text, cursor_offset) => {
                                let preedit = if text.is_empty() {
                                    None
                                } else {
                                    Some(Preedit::new(
                                        text,
                                        cursor_offset.map(|offset| offset.0),
                                    ))
                                };

                                if route.window.screen.ime.preedit() != preedit.as_ref() {
                                    route.window.screen.ime.set_preedit(preedit);
                                    route.redraw();
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
                }
                Event::WindowEvent {
                    event: winit::event::WindowEvent::Touch(touch),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        on_touch(route, touch);
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Focused(focused),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if self.config.hide_cursor_when_typing {
                            route.window.winit_window.set_cursor_visible(true);
                        }

                        let has_regained_focus = !route.window.is_focused && focused;
                        route.window.is_focused = focused;

                        if has_regained_focus {
                            route.redraw();
                        }

                        route.window.screen.on_focus_change(focused);
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Occluded(occluded),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        route.window.is_occluded = occluded;
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::ThemeChanged(new_theme),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        route.window.screen.update_config(
                            &self.config,
                            Some(new_theme),
                            &self.router.font_library,
                        );
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::DroppedFile(path),
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        if route.path == RoutePath::Assistant {
                            return;
                        }

                        let path: String = path.to_string_lossy().into();
                        route.window.screen.paste(&(path + " "), true);
                    }
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    window_id,
                    ..
                } => {
                    if new_size.width == 0 || new_size.height == 0 {
                        return;
                    }

                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        route.window.screen.resize(new_size);
                        route.window.screen.context_manager.schedule_render(60);
                    }
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            inner_size_writer: _,
                            scale_factor,
                        },
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        let scale = scale_factor as f32;
                        route
                            .window
                            .screen
                            .set_scale(scale, route.window.winit_window.inner_size());
                        route.redraw();
                    }
                }

                // Emitted when the event loop is being shut down.
                // This is irreversible - if this event is emitted, it is guaranteed to be the last event that gets emitted.
                // You generally want to treat this as an “do on quit” event.
                Event::LoopExiting { .. } => {
                    // TODO: Now we are forcing an exit operation
                    // but it should be revaluated since CloseRequested in MacOs
                    // not necessarily exit the process
                    std::process::exit(0);
                }

                Event::AboutToWait => {
                    // Update the scheduler after event processing to ensure
                    // the event loop deadline is as accurate as possible.
                    let control_flow = match scheduler.update() {
                        Some(instant) => ControlFlow::WaitUntil(instant),
                        None => ControlFlow::Wait,
                    };
                    event_loop_window_target.set_control_flow(control_flow);
                }

                Event::WindowEvent {
                    event: winit::event::WindowEvent::RedrawRequested,
                    window_id,
                    ..
                } => {
                    if let Some(route) = self.router.routes.get_mut(&window_id) {
                        // let start = std::time::Instant::now();

                        route.window.winit_window.pre_present_notify();
                        match route.path {
                            RoutePath::Assistant => {
                                route.window.screen.render_assistant(&route.assistant);
                            }
                            RoutePath::Welcome => {
                                route.window.screen.render_welcome();
                            }
                            RoutePath::Terminal => {
                                // Notify winit that we're about to present.
                                route.window.screen.render();
                            }
                            RoutePath::ConfirmQuit => {
                                route
                                    .window
                                    .screen
                                    .render_dialog("Do you want to leave Rio?");
                            }
                        }

                        // route.window.screen.render();
                        // let duration = start.elapsed();
                        // println!("Time elapsed in render() is: {:?}", duration);
                    }
                    // }
                    event_loop_window_target.set_control_flow(ControlFlow::Wait);
                }
                _ => {}
            }
        });

        Ok(())
    }
}
