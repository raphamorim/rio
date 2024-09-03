//! The keyboard input handling.

use std::sync::Mutex;
use std::time::Duration;

use calloop::timer::{TimeoutAction, Timer};
use calloop::{LoopHandle, RegistrationToken};
use tracing::warn;

use sctk::reexports::client::protocol::wl_keyboard::{
    Event as WlKeyboardEvent, KeyState as WlKeyState, KeymapFormat as WlKeymapFormat,
    WlKeyboard,
};
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

use crate::event::{ElementState, WindowEvent};
use crate::keyboard::ModifiersState;

use crate::platform_impl::common::xkb::Context;
use crate::platform_impl::wayland::event_loop::sink::EventSink;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId, WindowId};

impl Dispatch<WlKeyboard, KeyboardData, WinitState> for WinitState {
    fn event(
        state: &mut WinitState,
        wl_keyboard: &WlKeyboard,
        event: <WlKeyboard as Proxy>::Event,
        data: &KeyboardData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        let seat_state = match state.seats.get_mut(&data.seat.id()) {
            Some(seat_state) => seat_state,
            None => {
                warn!("Received keyboard event {event:?} without seat");
                return;
            }
        };
        let keyboard_state = match seat_state.keyboard_state.as_mut() {
            Some(keyboard_state) => keyboard_state,
            None => {
                warn!("Received keyboard event {event:?} without keyboard");
                return;
            }
        };

        match event {
            WlKeyboardEvent::Keymap { format, fd, size } => match format {
                WEnum::Value(format) => match format {
                    WlKeymapFormat::NoKeymap => {
                        warn!("non-xkb compatible keymap")
                    }
                    WlKeymapFormat::XkbV1 => {
                        let context = &mut keyboard_state.xkb_context;
                        context.set_keymap_from_fd(fd, size as usize);
                    }
                    _ => unreachable!(),
                },
                WEnum::Unknown(value) => {
                    warn!("unknown keymap format 0x{:x}", value)
                }
            },
            WlKeyboardEvent::Enter { surface, .. } => {
                let window_id = wayland::make_wid(&surface);

                // Mark the window as focused.
                let was_unfocused = match state.windows.get_mut().get(&window_id) {
                    Some(window) => {
                        let mut window = window.lock().unwrap();
                        let was_unfocused = !window.has_focus();
                        window.add_seat_focus(data.seat.id());
                        was_unfocused
                    }
                    None => return,
                };

                // Drop the repeat, if there were any.
                keyboard_state.current_repeat = None;
                if let Some(token) = keyboard_state.repeat_token.take() {
                    keyboard_state.loop_handle.remove(token);
                }

                *data.window_id.lock().unwrap() = Some(window_id);

                // The keyboard focus is considered as general focus.
                if was_unfocused {
                    state
                        .events_sink
                        .push_window_event(WindowEvent::Focused(true), window_id);
                }

                // HACK: this is just for GNOME not fixing their ordering issue of modifiers.
                if std::mem::take(&mut seat_state.modifiers_pending) {
                    state.events_sink.push_window_event(
                        WindowEvent::ModifiersChanged(seat_state.modifiers.into()),
                        window_id,
                    );
                }
            }
            WlKeyboardEvent::Leave { surface, .. } => {
                let window_id = wayland::make_wid(&surface);

                // NOTE: we should drop the repeat regardless whethere it was for the present
                // window of for the window which just went gone.
                keyboard_state.current_repeat = None;
                if let Some(token) = keyboard_state.repeat_token.take() {
                    keyboard_state.loop_handle.remove(token);
                }

                // NOTE: The check whether the window exists is essential as we might get a
                // nil surface, regardless of what protocol says.
                let focused = match state.windows.get_mut().get(&window_id) {
                    Some(window) => {
                        let mut window = window.lock().unwrap();
                        window.remove_seat_focus(&data.seat.id());
                        window.has_focus()
                    }
                    None => return,
                };

                // We don't need to update it above, because the next `Enter` will overwrite
                // anyway.
                *data.window_id.lock().unwrap() = None;

                if !focused {
                    // Notify that no modifiers are being pressed.
                    state.events_sink.push_window_event(
                        WindowEvent::ModifiersChanged(ModifiersState::empty().into()),
                        window_id,
                    );

                    state
                        .events_sink
                        .push_window_event(WindowEvent::Focused(false), window_id);
                }
            }
            WlKeyboardEvent::Key {
                key,
                state: WEnum::Value(WlKeyState::Pressed),
                ..
            } => {
                let key = key + 8;

                key_input(
                    keyboard_state,
                    &mut state.events_sink,
                    data,
                    key,
                    ElementState::Pressed,
                    false,
                );

                let delay = match keyboard_state.repeat_info {
                    RepeatInfo::Repeat { delay, .. } => delay,
                    RepeatInfo::Disable => return,
                };

                if !keyboard_state
                    .xkb_context
                    .keymap_mut()
                    .unwrap()
                    .key_repeats(key)
                {
                    return;
                }

                keyboard_state.current_repeat = Some(key);

                // NOTE terminate ongoing timer and start a new timer.

                if let Some(token) = keyboard_state.repeat_token.take() {
                    keyboard_state.loop_handle.remove(token);
                }

                let timer = Timer::from_duration(delay);
                let wl_keyboard = wl_keyboard.clone();
                keyboard_state.repeat_token = keyboard_state
                    .loop_handle
                    .insert_source(timer, move |_, _, state| {
                        // Required to handle the wakeups from the repeat sources.
                        state.dispatched_events = true;

                        let data = wl_keyboard.data::<KeyboardData>().unwrap();
                        let seat_state = match state.seats.get_mut(&data.seat.id()) {
                            Some(seat_state) => seat_state,
                            None => return TimeoutAction::Drop,
                        };

                        let keyboard_state = match seat_state.keyboard_state.as_mut() {
                            Some(keyboard_state) => keyboard_state,
                            None => return TimeoutAction::Drop,
                        };

                        // NOTE: The removed on event source is batched, but key change to `None`
                        // is instant.
                        let repeat_keycode = match keyboard_state.current_repeat {
                            Some(repeat_keycode) => repeat_keycode,
                            None => return TimeoutAction::Drop,
                        };

                        key_input(
                            keyboard_state,
                            &mut state.events_sink,
                            data,
                            repeat_keycode,
                            ElementState::Pressed,
                            true,
                        );

                        // NOTE: the gap could change dynamically while repeat is going.
                        match keyboard_state.repeat_info {
                            RepeatInfo::Repeat { gap, .. } => {
                                TimeoutAction::ToDuration(gap)
                            }
                            RepeatInfo::Disable => TimeoutAction::Drop,
                        }
                    })
                    .ok();
            }
            WlKeyboardEvent::Key {
                key,
                state: WEnum::Value(WlKeyState::Released),
                ..
            } => {
                let key = key + 8;

                key_input(
                    keyboard_state,
                    &mut state.events_sink,
                    data,
                    key,
                    ElementState::Released,
                    false,
                );

                if keyboard_state.repeat_info != RepeatInfo::Disable
                    && keyboard_state
                        .xkb_context
                        .keymap_mut()
                        .unwrap()
                        .key_repeats(key)
                    && Some(key) == keyboard_state.current_repeat
                {
                    keyboard_state.current_repeat = None;
                    if let Some(token) = keyboard_state.repeat_token.take() {
                        keyboard_state.loop_handle.remove(token);
                    }
                }
            }
            WlKeyboardEvent::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                let xkb_context = &mut keyboard_state.xkb_context;
                let xkb_state = match xkb_context.state_mut() {
                    Some(state) => state,
                    None => return,
                };

                xkb_state.update_modifiers(
                    mods_depressed,
                    mods_latched,
                    mods_locked,
                    0,
                    0,
                    group,
                );
                seat_state.modifiers = xkb_state.modifiers().into();

                // HACK: part of the workaround from `WlKeyboardEvent::Enter`.
                let window_id = match *data.window_id.lock().unwrap() {
                    Some(window_id) => window_id,
                    None => {
                        seat_state.modifiers_pending = true;
                        return;
                    }
                };

                state.events_sink.push_window_event(
                    WindowEvent::ModifiersChanged(seat_state.modifiers.into()),
                    window_id,
                );
            }
            WlKeyboardEvent::RepeatInfo { rate, delay } => {
                keyboard_state.repeat_info = if rate == 0 {
                    // Stop the repeat once we get a disable event.
                    keyboard_state.current_repeat = None;
                    if let Some(repeat_token) = keyboard_state.repeat_token.take() {
                        keyboard_state.loop_handle.remove(repeat_token);
                    }
                    RepeatInfo::Disable
                } else {
                    let gap = Duration::from_micros(1_000_000 / rate as u64);
                    let delay = Duration::from_millis(delay as u64);
                    RepeatInfo::Repeat { gap, delay }
                };
            }
            _ => unreachable!(),
        }
    }
}

/// The state of the keyboard on the current seat.
#[derive(Debug)]
pub struct KeyboardState {
    /// The underlying WlKeyboard.
    pub keyboard: WlKeyboard,

    /// Loop handle to handle key repeat.
    pub loop_handle: LoopHandle<'static, WinitState>,

    /// The state of the keyboard.
    pub xkb_context: Context,

    /// The information about the repeat rate obtained from the compositor.
    pub repeat_info: RepeatInfo,

    /// The token of the current handle inside the calloop's event loop.
    pub repeat_token: Option<RegistrationToken>,

    /// The current repeat raw key.
    pub current_repeat: Option<u32>,
}

impl KeyboardState {
    pub fn new(
        keyboard: WlKeyboard,
        loop_handle: LoopHandle<'static, WinitState>,
    ) -> Self {
        Self {
            keyboard,
            loop_handle,
            xkb_context: Context::new().unwrap(),
            repeat_info: RepeatInfo::default(),
            repeat_token: None,
            current_repeat: None,
        }
    }
}

impl Drop for KeyboardState {
    fn drop(&mut self) {
        if self.keyboard.version() >= 3 {
            self.keyboard.release();
        }

        if let Some(token) = self.repeat_token.take() {
            self.loop_handle.remove(token);
        }
    }
}

/// The rate at which a pressed key is repeated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatInfo {
    /// Keys will be repeated at the specified rate and delay.
    Repeat {
        /// The time between the key repeats.
        gap: Duration,

        /// Delay (in milliseconds) between a key press and the start of repetition.
        delay: Duration,
    },

    /// Keys should not be repeated.
    Disable,
}

impl Default for RepeatInfo {
    /// The default repeat rate is 25 keys per second with the delay of 200ms.
    ///
    /// The values are picked based on the default in various compositors and Xorg.
    fn default() -> Self {
        Self::Repeat {
            gap: Duration::from_millis(40),
            delay: Duration::from_millis(200),
        }
    }
}

/// Keyboard user data.
#[derive(Debug)]
pub struct KeyboardData {
    /// The currently focused window surface. Could be `None` on bugged compositors, like mutter.
    window_id: Mutex<Option<WindowId>>,

    /// The seat used to create this keyboard.
    seat: WlSeat,
}

impl KeyboardData {
    pub fn new(seat: WlSeat) -> Self {
        Self {
            window_id: Default::default(),
            seat,
        }
    }
}

fn key_input(
    keyboard_state: &mut KeyboardState,
    event_sink: &mut EventSink,
    data: &KeyboardData,
    keycode: u32,
    state: ElementState,
    repeat: bool,
) {
    let window_id = match *data.window_id.lock().unwrap() {
        Some(window_id) => window_id,
        None => return,
    };

    let device_id =
        crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
    if let Some(mut key_context) = keyboard_state.xkb_context.key_context() {
        let event = key_context.process_key_event(keycode, state, repeat);
        let event = WindowEvent::KeyboardInput {
            device_id,
            event,
            is_synthetic: false,
        };
        event_sink.push_window_event(event, window_id);
    }
}
