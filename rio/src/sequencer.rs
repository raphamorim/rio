#[cfg(target_os = "macos")]
use winit::platform::macos::WindowExtMacOS;

use crate::assistant::Assistant;
use crate::clipboard::ClipboardType;
use crate::event::{ClickState, EventP, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::{
    window::{configure_window, create_window_builder},
    Screen,
};
use crate::{utils::settings::create_settings_config, utils::watch::watch};
use colors::ColorRgb;
use std::collections::HashMap;
use std::error::Error;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event::{
    ElementState, Event, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{DeviceEvents, EventLoop, EventLoopWindowTarget};
use winit::platform::run_ondemand::EventLoopExtRunOnDemand;
use winit::window::{CursorIcon, Window, WindowId};

pub struct SequencerWindow {
    is_focused: bool,
    is_occluded: bool,
    window: Window,
    screen: Screen,
    #[cfg(target_os = "macos")]
    is_macos_deadzone: bool,
}

impl SequencerWindow {
    async fn new(
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
            window: winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        })
    }

    fn from_target(
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
            window: winit_window,
            screen,
            #[cfg(target_os = "macos")]
            is_macos_deadzone: false,
        }
    }
}

pub struct Sequencer {
    config: Rc<config::Config>,
    settings_config: Rc<config::Config>,
    windows: HashMap<WindowId, SequencerWindow>,
    window_config_editor: Option<WindowId>,
    event_proxy: Option<EventProxy>,
    assistant: Assistant,
}

impl Sequencer {
    pub fn new(
        config: config::Config,
        config_error: Option<config::ConfigError>,
    ) -> Sequencer {
        let mut assistant = Assistant::new();
        if let Some(error) = config_error {
            assistant.add(error.into());
        }

        let settings_config = Rc::new(create_settings_config(&config));
        Sequencer {
            config: Rc::new(config),
            settings_config,
            windows: HashMap::new(),
            event_proxy: None,
            window_config_editor: None,
            assistant,
        }
    }

    pub async fn run(
        &mut self,
        mut event_loop: EventLoop<EventP>,
    ) -> Result<(), Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        self.event_proxy = Some(EventProxy::new(proxy.clone()));
        let _ = watch(config::config_dir_path(), self.event_proxy.clone().unwrap());
        let mut scheduler = Scheduler::new(proxy);

        let seq_win = SequencerWindow::new(&event_loop, &self.config).await?;
        let first_window = seq_win.window.id();
        self.windows.insert(first_window, seq_win);

        event_loop.listen_device_events(DeviceEvents::Never);
        let _ = event_loop.run_ondemand(
            move |event, event_loop_window_target, control_flow| {
                match event {
                    Event::UserEvent(EventP {
                        payload, window_id, ..
                    }) => {
                        match payload {
                            RioEventType::Rio(RioEvent::Wakeup) => {
                                // Emitted when the application has been resumed.
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    sw.window.request_redraw();
                                }
                            }
                            RioEventType::Rio(RioEvent::Render) => {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    if self.config.disable_unfocused_render
                                        && !sw.is_focused
                                    {
                                        return;
                                    }
                                    sw.window.request_redraw();
                                }
                            }
                            RioEventType::Rio(RioEvent::ReportToAssistant(report)) => {
                                self.assistant.add(report);
                            }
                            RioEventType::Rio(RioEvent::UpdateConfig) => {
                                let config = config::Config::load();
                                self.config = config.into();
                                self.settings_config =
                                    create_settings_config(&self.config).into();
                                for (_id, sw) in self.windows.iter_mut() {
                                    sw.screen.update_config(&self.config);
                                    sw.window.request_redraw();
                                }
                            }
                            RioEventType::Rio(RioEvent::Exit) => {
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    if !sequencer_window.screen.try_close_existent_tab() {
                                        self.windows.remove(&window_id);

                                        if let Some(config_window_id) =
                                            self.window_config_editor
                                        {
                                            if config_window_id == window_id {
                                                self.window_config_editor = None;
                                            }
                                        }

                                        if self.windows.is_empty() {
                                            *control_flow =
                                                winit::event_loop::ControlFlow::Exit;
                                        }
                                    }
                                }
                            }
                            RioEventType::Rio(RioEvent::PrepareRender(millis)) => {
                                let timer_id = TimerId::new(Topic::Frame, 0);
                                let event = EventP::new(
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
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    sequencer_window.window.set_title(&title);
                                }
                            }
                            RioEventType::BlinkCursor
                            | RioEventType::BlinkCursorTimeout => {}
                            RioEventType::Rio(RioEvent::MouseCursorDirty) => {
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    sequencer_window.screen.reset_mouse();
                                }
                            }
                            RioEventType::Rio(RioEvent::Scroll(scroll)) => {
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    let mut terminal = sequencer_window
                                        .screen
                                        .ctx()
                                        .current()
                                        .terminal
                                        .lock();
                                    terminal.scroll_display(scroll);
                                    drop(terminal);
                                }
                            }
                            RioEventType::Rio(RioEvent::ClipboardLoad(
                                clipboard_type,
                                format,
                            )) => {
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    if sequencer_window.is_focused {
                                        let text = format(
                                            sequencer_window
                                                .screen
                                                .clipboard_get(clipboard_type)
                                                .as_str(),
                                        );
                                        sequencer_window
                                            .screen
                                            .ctx_mut()
                                            .current_mut()
                                            .messenger
                                            .send_bytes(text.into_bytes());
                                    }
                                }
                            }
                            RioEventType::Rio(RioEvent::PtyWrite(text)) => {
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    sequencer_window
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
                                // screen.colors()[index] or screen.state.colors[index]
                                if let Some(sequencer_window) =
                                    self.windows.get_mut(&window_id)
                                {
                                    let color =
                                        sequencer_window.screen.state.colors[index];
                                    let rgb = ColorRgb::from_color_arr(color);
                                    sequencer_window
                                        .screen
                                        .ctx_mut()
                                        .current_mut()
                                        .messenger
                                        .send_bytes(format(rgb).into_bytes());
                                }
                            }
                            RioEventType::Rio(RioEvent::CreateWindow) => {
                                let sw = SequencerWindow::from_target(
                                    event_loop_window_target,
                                    self.event_proxy.clone().unwrap(),
                                    &self.config,
                                    "Rio",
                                    None,
                                );
                                self.windows.insert(sw.window.id(), sw);
                            }
                            #[cfg(target_os = "macos")]
                            RioEventType::Rio(RioEvent::CreateNativeTab) => {
                                if let Some(current_sw) = self.windows.get_mut(&window_id)
                                {
                                    current_sw.window.request_redraw();

                                    let sw = SequencerWindow::from_target(
                                        event_loop_window_target,
                                        self.event_proxy.clone().unwrap(),
                                        &self.config,
                                        "zsh",
                                        Some(current_sw.window.tabbing_identifier()),
                                    );

                                    self.windows.insert(sw.window.id(), sw);
                                }
                            }
                            RioEventType::Rio(RioEvent::CreateConfigEditor) => {
                                if let Some(config_editor_window_id) =
                                    self.window_config_editor
                                {
                                    if let Some(sequencer_window) =
                                        self.windows.get_mut(&config_editor_window_id)
                                    {
                                        sequencer_window.window.focus_window();
                                    }
                                } else {
                                    let sw = SequencerWindow::from_target(
                                        event_loop_window_target,
                                        self.event_proxy.clone().unwrap(),
                                        &self.settings_config,
                                        "Rio Configuration",
                                        None,
                                    );
                                    let window_id = sw.window.id();
                                    self.windows.insert(window_id, sw);
                                    self.window_config_editor = Some(window_id);
                                }
                            }
                            #[cfg(target_os = "macos")]
                            RioEventType::Rio(RioEvent::CloseWindow) => {
                                if let Some(current_sw) = self.windows.get_mut(&window_id)
                                {
                                    if current_sw.window.num_tabs() > 1 {
                                        self.windows.remove(&window_id);
                                    }
                                }
                            }
                            #[cfg(target_os = "macos")]
                            RioEventType::Rio(RioEvent::SelectNativeTabByIndex(
                                tab_index,
                            )) => {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    sw.window.select_tab_at_index(tab_index);
                                }
                            }
                            #[cfg(target_os = "macos")]
                            RioEventType::Rio(RioEvent::SelectNativeTabLast) => {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    sw.window
                                        .select_tab_at_index(sw.window.num_tabs() - 1);
                                }
                            }
                            #[cfg(target_os = "macos")]
                            RioEventType::Rio(RioEvent::SelectNativeTabNext) => {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    sw.window.select_next_tab();
                                }
                            }
                            #[cfg(target_os = "macos")]
                            RioEventType::Rio(RioEvent::SelectNativeTabPrev) => {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    sw.window.select_previous_tab();
                                }
                            }
                            RioEventType::Rio(RioEvent::Minimize(set_minimize)) => {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    sw.window.set_minimized(set_minimize);
                                }
                            }
                            _ => {}
                        }
                    }
                    Event::Resumed => {}

                    Event::WindowEvent {
                        event: winit::event::WindowEvent::CloseRequested,
                        window_id,
                        ..
                    } => {
                        self.windows.remove(&window_id);

                        if let Some(config_window_id) = self.window_config_editor {
                            if config_window_id == window_id {
                                self.window_config_editor = None;
                            }
                        }

                        if self.windows.is_empty() {
                            *control_flow = winit::event_loop::ControlFlow::Exit;
                        }
                    }

                    Event::WindowEvent {
                        event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                        window_id,
                        ..
                    } => {
                        if let Some(sequencer_window) = self.windows.get_mut(&window_id) {
                            sequencer_window.screen.set_modifiers(modifiers);
                        }
                    }

                    Event::WindowEvent {
                        event: WindowEvent::MouseInput { state, button, .. },
                        window_id,
                        ..
                    } => {
                        if let Some(sequencer_window) = self.windows.get_mut(&window_id) {
                            sequencer_window.window.set_cursor_visible(true);

                            match button {
                                MouseButton::Left => {
                                    sequencer_window.screen.mouse.left_button_state =
                                        state
                                }
                                MouseButton::Middle => {
                                    sequencer_window.screen.mouse.middle_button_state =
                                        state
                                }
                                MouseButton::Right => {
                                    sequencer_window.screen.mouse.right_button_state =
                                        state
                                }
                                _ => (),
                            }

                            #[cfg(target_os = "macos")]
                            {
                                if sequencer_window.is_macos_deadzone {
                                    return;
                                }
                            }

                            match state {
                                ElementState::Pressed => {
                                    // Process mouse press before bindings to update the `click_state`.
                                    if !sequencer_window
                                        .screen
                                        .modifiers
                                        .state()
                                        .shift_key()
                                        && sequencer_window.screen.mouse_mode()
                                    {
                                        sequencer_window.screen.mouse.click_state =
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

                                        sequencer_window
                                            .screen
                                            .mouse_report(code, ElementState::Pressed);

                                        sequencer_window
                                            .screen
                                            .process_mouse_bindings(button);
                                    } else {
                                        // Calculate time since the last click to handle double/triple clicks.
                                        let now = Instant::now();
                                        let elapsed = now
                                            - sequencer_window
                                                .screen
                                                .mouse
                                                .last_click_timestamp;
                                        sequencer_window
                                            .screen
                                            .mouse
                                            .last_click_timestamp = now;

                                        let threshold = Duration::from_millis(300);
                                        let mouse = &sequencer_window.screen.mouse;
                                        sequencer_window.screen.mouse.click_state =
                                            match mouse.click_state {
                                                // Reset click state if button has changed.
                                                _ if button
                                                    != mouse.last_click_button =>
                                                {
                                                    sequencer_window
                                                        .screen
                                                        .mouse
                                                        .last_click_button = button;
                                                    ClickState::Click
                                                }
                                                ClickState::Click
                                                    if elapsed < threshold =>
                                                {
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
                                            sequencer_window.screen.display_offset();

                                        if let MouseButton::Left = button {
                                            let point = sequencer_window
                                                .screen
                                                .mouse_position(display_offset);
                                            sequencer_window.screen.on_left_click(point);
                                        }

                                        sequencer_window.window.request_redraw();
                                    }
                                    // sequencer_window.screen.process_mouse_bindings(button);
                                }
                                ElementState::Released => {
                                    if !sequencer_window
                                        .screen
                                        .modifiers
                                        .state()
                                        .shift_key()
                                        && sequencer_window.screen.mouse_mode()
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
                                        sequencer_window
                                            .screen
                                            .mouse_report(code, ElementState::Released);
                                        return;
                                    }

                                    if let MouseButton::Left | MouseButton::Right = button
                                    {
                                        // Copy selection on release, to prevent flooding the display server.
                                        sequencer_window
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
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            sw.window.set_cursor_visible(true);

                            let x = position.x;
                            let y = position.y;

                            let lmb_pressed = sw.screen.mouse.left_button_state
                                == ElementState::Pressed;
                            let rmb_pressed = sw.screen.mouse.right_button_state
                                == ElementState::Pressed;

                            let has_selection = !sw.screen.selection_is_empty();

                            #[cfg(target_os = "macos")]
                            {
                                // Dead zone for MacOS only
                                // e.g: Dragging the terminal
                                if !has_selection
                                    && !sw.screen.context_manager.config.is_native
                                    && sw.screen.is_macos_deadzone(y)
                                {
                                    if sw.screen.is_macos_deadzone_draggable(x) {
                                        if lmb_pressed || rmb_pressed {
                                            sw.screen.clear_selection();
                                            sw.window
                                                .set_cursor_icon(CursorIcon::Grabbing);
                                        } else {
                                            sw.window.set_cursor_icon(CursorIcon::Grab);
                                        }
                                    } else {
                                        sw.window.set_cursor_icon(CursorIcon::Default);
                                    }

                                    sw.is_macos_deadzone = true;
                                    return;
                                }

                                sw.is_macos_deadzone = false;
                            }

                            let cursor_icon = if !sw.screen.modifiers.state().shift_key()
                                && sw.screen.mouse_mode()
                            {
                                CursorIcon::Default
                            } else {
                                CursorIcon::Text
                            };

                            sw.window.set_cursor_icon(cursor_icon);
                            if has_selection && (lmb_pressed || rmb_pressed) {
                                sw.screen.update_selection_scrolling(y);
                            }

                            let display_offset = sw.screen.display_offset();
                            let old_point = sw.screen.mouse_position(display_offset);

                            let x = x.clamp(0.0, sw.screen.sugarloaf.layout.width.into())
                                as usize;
                            let y = y.clamp(0.0, sw.screen.sugarloaf.layout.height.into())
                                as usize;
                            sw.screen.mouse.x = x;
                            sw.screen.mouse.y = y;

                            let point = sw.screen.mouse_position(display_offset);
                            let square_changed = old_point != point;

                            let inside_text_area = sw.screen.contains_point(x, y);
                            let square_side = sw.screen.side_by_pos(x);

                            // If the mouse hasn't changed cells, do nothing.
                            if !square_changed
                                && sw.screen.mouse.square_side == square_side
                                && sw.screen.mouse.inside_text_area == inside_text_area
                            {
                                return;
                            }

                            sw.screen.mouse.inside_text_area = inside_text_area;
                            sw.screen.mouse.square_side = square_side;

                            if (lmb_pressed || rmb_pressed)
                                && (sw.screen.modifiers.state().shift_key()
                                    || !sw.screen.mouse_mode())
                            {
                                sw.screen.update_selection(point, square_side);
                                sw.window.request_redraw();
                            } else if square_changed
                                && sw.screen.has_mouse_motion_and_drag()
                            {
                                if lmb_pressed {
                                    sw.screen.mouse_report(32, ElementState::Pressed);
                                } else if sw.screen.mouse.middle_button_state
                                    == ElementState::Pressed
                                {
                                    sw.screen.mouse_report(33, ElementState::Pressed);
                                } else if sw.screen.mouse.right_button_state
                                    == ElementState::Pressed
                                {
                                    sw.screen.mouse_report(34, ElementState::Pressed);
                                } else if sw.screen.has_mouse_motion() {
                                    sw.screen.mouse_report(35, ElementState::Pressed);
                                }
                            }
                        }
                    }

                    Event::WindowEvent {
                        event: WindowEvent::MouseWheel { delta, phase, .. },
                        window_id,
                        ..
                    } => {
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            sw.window.set_cursor_visible(true);
                            match delta {
                                MouseScrollDelta::LineDelta(columns, lines) => {
                                    let new_scroll_px_x =
                                        columns * sw.screen.sugarloaf.layout.font_size;
                                    let new_scroll_px_y =
                                        lines * sw.screen.sugarloaf.layout.font_size;
                                    sw.screen.scroll(
                                        new_scroll_px_x as f64,
                                        new_scroll_px_y as f64,
                                    );
                                }
                                MouseScrollDelta::PixelDelta(mut lpos) => {
                                    match phase {
                                        TouchPhase::Started => {
                                            // Reset offset to zero.
                                            sw.screen.mouse.accumulated_scroll =
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

                                            sw.screen.scroll(lpos.x, lpos.y);
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
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            match key_event.state {
                                ElementState::Pressed => {
                                    sw.window.set_cursor_visible(false);

                                    sw.screen.process_key_event(&key_event);
                                }

                                ElementState::Released => {
                                    sw.window.request_redraw();
                                }
                            }
                        }
                    }

                    Event::WindowEvent {
                        event: WindowEvent::Ime(ime),
                        window_id,
                        ..
                    } => {
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            match ime {
                                Ime::Commit(text) => {
                                    // Don't use bracketed paste for single char input.
                                    sw.screen.paste(&text, text.chars().count() > 1);
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

                                    if sw.screen.ime.preedit() != preedit.as_ref() {
                                        sw.screen.ime.set_preedit(preedit);
                                        sw.window.request_redraw();
                                    }
                                }
                                Ime::Enabled => {
                                    sw.screen.ime.set_enabled(true);
                                }
                                Ime::Disabled => {
                                    sw.screen.ime.set_enabled(false);
                                }
                            }
                        }
                    }

                    Event::WindowEvent {
                        event: winit::event::WindowEvent::Focused(focused),
                        window_id,
                        ..
                    } => {
                        if let Some(sequencer_window) = self.windows.get_mut(&window_id) {
                            sequencer_window.window.set_cursor_visible(true);
                            sequencer_window.is_focused = focused;
                        }
                    }

                    Event::WindowEvent {
                        event: winit::event::WindowEvent::Occluded(occluded),
                        window_id,
                        ..
                    } => {
                        if let Some(sequencer_window) = self.windows.get_mut(&window_id) {
                            sequencer_window.is_occluded = occluded;
                        }
                    }

                    Event::WindowEvent {
                        event: winit::event::WindowEvent::DroppedFile(path),
                        window_id,
                        ..
                    } => {
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            let path: String = path.to_string_lossy().into();
                            sw.screen.paste(&(path + " "), true);
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

                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            sw.screen.resize(new_size);
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
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            sw.screen
                                .set_scale(scale_factor as f32, sw.window.inner_size());
                            sw.window.request_redraw();
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

                    Event::RedrawRequested(window_id) => {
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            // let start = std::time::Instant::now();

                            #[cfg(target_os = "macos")]
                            {
                                if sw.screen.context_manager.config.is_native {
                                    sw.screen.update_top_y_for_native_tabs(
                                        sw.window.num_tabs(),
                                    );
                                }
                            }

                            if !self.assistant.inner.is_empty() {
                                sw.screen.render_assistant(&self.assistant);
                            } else {
                                sw.screen.render();
                            }
                            // let duration = start.elapsed();
                            // println!("Time elapsed in render() is: {:?}", duration);
                        }
                        // }

                        scheduler.update();
                        *control_flow = winit::event_loop::ControlFlow::Wait;
                    }
                    _ => {}
                }
            },
        );

        Ok(())
    }
}
