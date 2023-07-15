#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use {
    wayland_client::Display as WaylandDisplay,
    winit::platform::wayland::EventLoopWindowTargetExtWayland,
};

use crate::clipboard::ClipboardType;
use crate::event::{ClickState, EventP, EventProxy, RioEvent, RioEventType};
use crate::ime::Preedit;
use crate::scheduler::{Scheduler, TimerId, Topic};
use crate::screen::{
    window::{configure_window, create_window_builder},
    Screen,
};
use crate::utils::watch::watch;
use colors::ColorRgb;
use std::collections::HashMap;
use std::error::Error;
use std::os::raw::c_void;
use std::rc::Rc;
use std::time::{Duration, Instant};
use winit::event::{
    ElementState, Event, Ime, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent,
};
use winit::event_loop::{DeviceEventFilter, EventLoop, EventLoopWindowTarget};
use winit::platform::run_return::EventLoopExtRunReturn;
use winit::window::{CursorIcon, Window, WindowId};

pub struct SequencerWindow {
    is_focused: bool,
    is_occluded: bool,
    window: Window,
    screen: Screen,
    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    has_wayland_forcefully_reloaded: bool,
}

impl SequencerWindow {
    async fn new(
        event_loop: &EventLoop<EventP>,
        config: &Rc<config::Config>,
    ) -> Result<Self, Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let event_proxy = EventProxy::new(proxy.clone());
        let window_builder = create_window_builder("Rio");
        let winit_window = window_builder.build(event_loop).unwrap();
        let winit_window = configure_window(winit_window, config);

        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let display: Option<*mut c_void> = event_loop.wayland_display();
        #[cfg(any(not(feature = "wayland"), target_os = "macos", windows))]
        let display: Option<*mut c_void> = Option::None;

        let mut screen = Screen::new(&winit_window, config, event_proxy, display).await?;

        screen.init(config.colors.background.1);

        Ok(Self {
            is_focused: false,
            is_occluded: false,
            window: winit_window,
            screen,
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            has_wayland_forcefully_reloaded: false,
        })
    }

    fn from_target(
        event_loop: &EventLoopWindowTarget<EventP>,
        event_proxy: EventProxy,
        config: &Rc<config::Config>,
        window_name: &str,
    ) -> Self {
        let window_builder = create_window_builder(window_name);
        let winit_window = window_builder.build(event_loop).unwrap();
        let winit_window = configure_window(winit_window, config);

        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let display: Option<*mut c_void> = event_loop.wayland_display();
        #[cfg(any(not(feature = "wayland"), target_os = "macos", windows))]
        let display: Option<*mut c_void> = Option::None;

        let mut screen = futures::executor::block_on(Screen::new(
            &winit_window,
            config,
            event_proxy,
            display,
        ))
        .expect("Screen not created");

        screen.init(config.colors.background.1);

        Self {
            is_focused: false,
            is_occluded: false,
            window: winit_window,
            screen,
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            has_wayland_forcefully_reloaded: false,
        }
    }
}

pub struct Sequencer {
    config: Rc<config::Config>,
    editor_config: Rc<config::Config>,
    windows: HashMap<WindowId, SequencerWindow>,
    window_config_editor: Option<WindowId>,
    has_updates: Vec<WindowId>,
    event_proxy: Option<EventProxy>,
}

impl Sequencer {
    pub fn new(config: config::Config) -> Sequencer {
        let mut editor_config = config.clone();
        #[cfg(target_os = "macos")]
        let fallback = String::from("vim");
        #[cfg(not(target_os = "macos"))]
        let fallback = String::from("vi");

        let editor = std::env::var("EDITOR").unwrap_or(fallback);
        let editor_program = config::Shell {
            program: editor,
            args: vec![config::config_file_path()],
        };
        editor_config.shell = editor_program;
        Sequencer {
            config: Rc::new(config),
            editor_config: Rc::new(editor_config),
            windows: HashMap::new(),
            has_updates: vec![],
            event_proxy: None,
            window_config_editor: None,
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

        #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        let mut wayland_event_queue = event_loop.wayland_display().map(|display| {
            let display = unsafe { WaylandDisplay::from_external_display(display as _) };
            display.create_event_queue()
        });

        // #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
        // let _wayland_surface = if event_loop.is_wayland() {
        //     // Attach surface to Rio internal wayland queue to handle frame callbacks.
        //     let surface = winit_window.wayland_surface().unwrap();
        //     let proxy: Proxy<WlSurface> = unsafe { Proxy::from_c_ptr(surface as _) };
        //     Some(proxy.attach(wayland_event_queue.as_ref().unwrap().token()))
        // } else {
        //     None
        // };

        let seq_win = SequencerWindow::new(&event_loop, &self.config).await?;
        let first_window = seq_win.window.id();
        self.windows.insert(first_window, seq_win);

        event_loop.set_device_event_filter(DeviceEventFilter::Always);
        event_loop.run_return(move |event, event_loop_window_target, control_flow| {
            match event {
                Event::UserEvent(EventP {
                    payload, window_id, ..
                }) => {
                    match payload {
                        RioEventType::Rio(RioEvent::Wakeup) => {
                            // Emitted when the application has been resumed.
                            // This is a hack to avoid an odd scenario in wayland window initialization
                            // wayland windows starts with the wrong width/height.
                            // Rio is ignoring wayland new dimension events, so the terminal
                            // start with the wrong width/height (fix the ignore would be the best fix though)
                            //
                            // The code below forcefully reload dimensions in the terminal initialization
                            // to load current width/height.
                            #[cfg(all(
                                feature = "wayland",
                                not(any(target_os = "macos", windows))
                            ))]
                            {
                                if let Some(sw) = self.windows.get_mut(&window_id) {
                                    if !sw.has_wayland_forcefully_reloaded {
                                        sw.screen.update_config(&self.config);
                                        sw.has_wayland_forcefully_reloaded = true;
                                    }
                                }
                            }

                            if !self.has_updates.contains(&window_id) {
                                self.has_updates.push(window_id);
                            }
                        }
                        RioEventType::Rio(RioEvent::Render) => {
                            if let Some(sw) = self.windows.get_mut(&window_id) {
                                if self.config.disable_unfocused_render && !sw.is_focused
                                {
                                    return;
                                }
                                if !self.has_updates.contains(&window_id) {
                                    self.has_updates.push(window_id);
                                }
                            }
                        }
                        RioEventType::Rio(RioEvent::UpdateConfig) => {
                            for (_id, sw) in self.windows.iter_mut() {
                                let config = config::Config::load();
                                self.config = config.into();
                                sw.screen.update_config(&self.config);
                                if !self.has_updates.contains(&window_id) {
                                    self.has_updates.push(window_id);
                                }
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
                        RioEventType::Rio(RioEvent::Title(_title)) => {
                            // if !self.ctx.preserve_title && self.ctx.config.window.dynamic_title {
                            // self.ctx.window().set_title(title);
                            // }
                        }
                        RioEventType::BlinkCursor | RioEventType::BlinkCursorTimeout => {}
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
                                let color = sequencer_window.screen.state.colors[index];
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
                            );
                            self.windows.insert(sw.window.id(), sw);
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
                                    &self.editor_config,
                                    "Rio Configuration",
                                );
                                let window_id = sw.window.id();
                                self.windows.insert(window_id, sw);
                                self.window_config_editor = Some(window_id);
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
                                sequencer_window.screen.mouse.left_button_state = state
                            }
                            MouseButton::Middle => {
                                sequencer_window.screen.mouse.middle_button_state = state
                            }
                            MouseButton::Right => {
                                sequencer_window.screen.mouse.right_button_state = state
                            }
                            _ => (),
                        }

                        match state {
                            ElementState::Pressed => {
                                // Process mouse press before bindings to update the `click_state`.
                                if !sequencer_window.screen.modifiers.shift()
                                    && sequencer_window.screen.mouse_mode()
                                {
                                    sequencer_window.screen.mouse.click_state =
                                        ClickState::None;

                                    let code = match button {
                                        MouseButton::Left => 0,
                                        MouseButton::Middle => 1,
                                        MouseButton::Right => 2,
                                        // Can't properly report more than three buttons..
                                        MouseButton::Other(_) => return,
                                    };

                                    sequencer_window
                                        .screen
                                        .mouse_report(code, ElementState::Pressed);
                                } else {
                                    // Calculate time since the last click to handle double/triple clicks.
                                    let now = Instant::now();
                                    let elapsed = now
                                        - sequencer_window
                                            .screen
                                            .mouse
                                            .last_click_timestamp;
                                    sequencer_window.screen.mouse.last_click_timestamp =
                                        now;

                                    let threshold = Duration::from_millis(300);
                                    let mouse = &sequencer_window.screen.mouse;
                                    sequencer_window.screen.mouse.click_state =
                                        match mouse.click_state {
                                            // Reset click state if button has changed.
                                            _ if button != mouse.last_click_button => {
                                                sequencer_window
                                                    .screen
                                                    .mouse
                                                    .last_click_button = button;
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
                                        sequencer_window.screen.display_offset();

                                    if let MouseButton::Left = button {
                                        let point = sequencer_window
                                            .screen
                                            .mouse_position(display_offset);
                                        sequencer_window.screen.on_left_click(point);
                                    }

                                    if !self.has_updates.contains(&window_id) {
                                        self.has_updates.push(window_id);
                                    }
                                }
                                // sequencer_window.screen.process_mouse_bindings(button);
                            }
                            ElementState::Released => {
                                if !sequencer_window.screen.modifiers.shift()
                                    && sequencer_window.screen.mouse_mode()
                                {
                                    let code = match button {
                                        MouseButton::Left => 0,
                                        MouseButton::Middle => 1,
                                        MouseButton::Right => 2,
                                        // Can't properly report more than three buttons.
                                        MouseButton::Other(_) => return,
                                    };
                                    sequencer_window
                                        .screen
                                        .mouse_report(code, ElementState::Released);
                                    return;
                                }

                                if let MouseButton::Left | MouseButton::Right = button {
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

                        let lmb_pressed =
                            sw.screen.mouse.left_button_state == ElementState::Pressed;
                        let rmb_pressed =
                            sw.screen.mouse.right_button_state == ElementState::Pressed;

                        if !sw.screen.selection_is_empty() && (lmb_pressed || rmb_pressed)
                        {
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

                        let cursor_icon =
                            if !sw.screen.modifiers.shift() && sw.screen.mouse_mode() {
                                CursorIcon::Default
                            } else {
                                CursorIcon::Text
                            };

                        sw.window.set_cursor_icon(cursor_icon);

                        if (lmb_pressed || rmb_pressed)
                            && (sw.screen.modifiers.shift() || !sw.screen.mouse_mode())
                        {
                            sw.screen.update_selection(point, square_side);
                        } else if square_changed && sw.screen.has_mouse_motion_and_drag()
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

                        if !self.has_updates.contains(&window_id) {
                            self.has_updates.push(window_id);
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
                    event: winit::event::WindowEvent::ReceivedCharacter(character),
                    window_id,
                    ..
                } => {
                    if let Some(sw) = self.windows.get_mut(&window_id) {
                        sw.screen.input_character(character);
                    }
                }

                Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::KeyboardInput {
                            is_synthetic: false,
                            input:
                                winit::event::KeyboardInput {
                                    virtual_keycode,
                                    scancode,
                                    state,
                                    ..
                                },
                            ..
                        },
                    window_id,
                    ..
                } => match state {
                    ElementState::Pressed => {
                        if let Some(sw) = self.windows.get_mut(&window_id) {
                            sw.window.set_cursor_visible(false);
                            sw.screen.input_keycode(virtual_keycode, scancode);
                        }
                    }

                    ElementState::Released => {
                        if !self.has_updates.contains(&window_id) {
                            self.has_updates.push(window_id);
                        }
                    }
                },

                Event::WindowEvent {
                    event: WindowEvent::Ime(ime),
                    window_id,
                    ..
                } => {
                    if let Some(sw) = self.windows.get_mut(&window_id) {
                        match ime {
                            Ime::Commit(text) => {
                                sw.screen.paste(&text, true);
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
                                    sw.screen.render();
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
                            new_inner_size,
                            scale_factor,
                        },
                    window_id,
                    ..
                } => {
                    if let Some(sw) = self.windows.get_mut(&window_id) {
                        sw.screen.set_scale(scale_factor as f32, *new_inner_size);
                        if !self.has_updates.contains(&window_id) {
                            self.has_updates.push(window_id);
                        }
                    }
                }

                // Emitted when the event loop is being shut down.
                // This is irreversible - if this event is emitted, it is guaranteed to be the last event that gets emitted.
                // You generally want to treat this as an “do on quit” event.
                Event::LoopDestroyed { .. } => {
                    // TODO: Now we are forcing an exit operation
                    // but it should be revaluated since CloseRequested in MacOs
                    // not necessarily exit the process
                    std::process::exit(0);
                }
                Event::RedrawEventsCleared { .. } => {
                    // Skip render for macos and x11 windows that are fully occluded
                    #[cfg(all(
                        feature = "wayland",
                        not(any(target_os = "macos", target_os = "windows"))
                    ))]
                    if let Some(w_event_queue) = wayland_event_queue.as_mut() {
                        w_event_queue
                            .dispatch_pending(&mut (), |_, _, _| {})
                            .expect("failed to dispatch wayland event queue");
                    }

                    if !self.has_updates.is_empty() {
                        for window_id in self.has_updates.iter() {
                            if let Some(sw) = self.windows.get_mut(window_id) {
                                sw.screen.render();
                            }
                        }

                        self.has_updates = vec![];
                    }

                    scheduler.update();
                }
                Event::MainEventsCleared { .. } => {}
                Event::RedrawRequested(_window_id) => {
                    *control_flow = winit::event_loop::ControlFlow::Wait;
                }
                _ => {}
            }
        });

        Ok(())
    }
}
