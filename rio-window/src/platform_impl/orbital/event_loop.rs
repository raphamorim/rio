use std::cell::Cell;
use std::collections::VecDeque;
use std::marker::PhantomData;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Instant;
use std::{mem, slice};

use bitflags::bitflags;
use orbclient::{
    ButtonEvent, EventOption, FocusEvent, HoverEvent, KeyEvent, MouseEvent,
    MouseRelativeEvent, MoveEvent, QuitEvent, ResizeEvent, ScrollEvent, TextInputEvent,
};
use smol_str::SmolStr;

use crate::error::EventLoopError;
use crate::event::{self, Ime, Modifiers, StartCause};
use crate::event_loop::{self, ControlFlow, DeviceEvents};
use crate::keyboard::{
    Key, KeyCode, KeyLocation, ModifiersKeys, ModifiersState, NamedKey, NativeKey,
    NativeKeyCode, PhysicalKey,
};
use crate::window::{
    CustomCursor as RootCustomCursor, CustomCursorSource, WindowId as RootWindowId,
};

use super::{
    DeviceId, KeyEventExtra, MonitorHandle, OsError, PlatformSpecificEventLoopAttributes,
    RedoxSocket, TimeSocket, WindowId, WindowProperties,
};

fn convert_scancode(scancode: u8) -> (PhysicalKey, Option<NamedKey>) {
    // Key constants from https://docs.rs/orbclient/latest/orbclient/event/index.html
    let (key_code, named_key_opt) = match scancode {
        orbclient::K_A => (KeyCode::KeyA, None),
        orbclient::K_B => (KeyCode::KeyB, None),
        orbclient::K_C => (KeyCode::KeyC, None),
        orbclient::K_D => (KeyCode::KeyD, None),
        orbclient::K_E => (KeyCode::KeyE, None),
        orbclient::K_F => (KeyCode::KeyF, None),
        orbclient::K_G => (KeyCode::KeyG, None),
        orbclient::K_H => (KeyCode::KeyH, None),
        orbclient::K_I => (KeyCode::KeyI, None),
        orbclient::K_J => (KeyCode::KeyJ, None),
        orbclient::K_K => (KeyCode::KeyK, None),
        orbclient::K_L => (KeyCode::KeyL, None),
        orbclient::K_M => (KeyCode::KeyM, None),
        orbclient::K_N => (KeyCode::KeyN, None),
        orbclient::K_O => (KeyCode::KeyO, None),
        orbclient::K_P => (KeyCode::KeyP, None),
        orbclient::K_Q => (KeyCode::KeyQ, None),
        orbclient::K_R => (KeyCode::KeyR, None),
        orbclient::K_S => (KeyCode::KeyS, None),
        orbclient::K_T => (KeyCode::KeyT, None),
        orbclient::K_U => (KeyCode::KeyU, None),
        orbclient::K_V => (KeyCode::KeyV, None),
        orbclient::K_W => (KeyCode::KeyW, None),
        orbclient::K_X => (KeyCode::KeyX, None),
        orbclient::K_Y => (KeyCode::KeyY, None),
        orbclient::K_Z => (KeyCode::KeyZ, None),
        orbclient::K_0 => (KeyCode::Digit0, None),
        orbclient::K_1 => (KeyCode::Digit1, None),
        orbclient::K_2 => (KeyCode::Digit2, None),
        orbclient::K_3 => (KeyCode::Digit3, None),
        orbclient::K_4 => (KeyCode::Digit4, None),
        orbclient::K_5 => (KeyCode::Digit5, None),
        orbclient::K_6 => (KeyCode::Digit6, None),
        orbclient::K_7 => (KeyCode::Digit7, None),
        orbclient::K_8 => (KeyCode::Digit8, None),
        orbclient::K_9 => (KeyCode::Digit9, None),

        orbclient::K_ALT => (KeyCode::AltLeft, Some(NamedKey::Alt)),
        orbclient::K_ALT_GR => (KeyCode::AltRight, Some(NamedKey::AltGraph)),
        orbclient::K_BACKSLASH => (KeyCode::Backslash, None),
        orbclient::K_BKSP => (KeyCode::Backspace, Some(NamedKey::Backspace)),
        orbclient::K_BRACE_CLOSE => (KeyCode::BracketRight, None),
        orbclient::K_BRACE_OPEN => (KeyCode::BracketLeft, None),
        orbclient::K_CAPS => (KeyCode::CapsLock, Some(NamedKey::CapsLock)),
        orbclient::K_COMMA => (KeyCode::Comma, None),
        orbclient::K_CTRL => (KeyCode::ControlLeft, Some(NamedKey::Control)),
        orbclient::K_DEL => (KeyCode::Delete, Some(NamedKey::Delete)),
        orbclient::K_DOWN => (KeyCode::ArrowDown, Some(NamedKey::ArrowDown)),
        orbclient::K_END => (KeyCode::End, Some(NamedKey::End)),
        orbclient::K_ENTER => (KeyCode::Enter, Some(NamedKey::Enter)),
        orbclient::K_EQUALS => (KeyCode::Equal, None),
        orbclient::K_ESC => (KeyCode::Escape, Some(NamedKey::Escape)),
        orbclient::K_F1 => (KeyCode::F1, Some(NamedKey::F1)),
        orbclient::K_F2 => (KeyCode::F2, Some(NamedKey::F2)),
        orbclient::K_F3 => (KeyCode::F3, Some(NamedKey::F3)),
        orbclient::K_F4 => (KeyCode::F4, Some(NamedKey::F4)),
        orbclient::K_F5 => (KeyCode::F5, Some(NamedKey::F5)),
        orbclient::K_F6 => (KeyCode::F6, Some(NamedKey::F6)),
        orbclient::K_F7 => (KeyCode::F7, Some(NamedKey::F7)),
        orbclient::K_F8 => (KeyCode::F8, Some(NamedKey::F8)),
        orbclient::K_F9 => (KeyCode::F9, Some(NamedKey::F9)),
        orbclient::K_F10 => (KeyCode::F10, Some(NamedKey::F10)),
        orbclient::K_F11 => (KeyCode::F11, Some(NamedKey::F11)),
        orbclient::K_F12 => (KeyCode::F12, Some(NamedKey::F12)),
        orbclient::K_HOME => (KeyCode::Home, Some(NamedKey::Home)),
        orbclient::K_LEFT => (KeyCode::ArrowLeft, Some(NamedKey::ArrowLeft)),
        orbclient::K_LEFT_SHIFT => (KeyCode::ShiftLeft, Some(NamedKey::Shift)),
        orbclient::K_MINUS => (KeyCode::Minus, None),
        orbclient::K_NUM_0 => (KeyCode::Numpad0, None),
        orbclient::K_NUM_1 => (KeyCode::Numpad1, None),
        orbclient::K_NUM_2 => (KeyCode::Numpad2, None),
        orbclient::K_NUM_3 => (KeyCode::Numpad3, None),
        orbclient::K_NUM_4 => (KeyCode::Numpad4, None),
        orbclient::K_NUM_5 => (KeyCode::Numpad5, None),
        orbclient::K_NUM_6 => (KeyCode::Numpad6, None),
        orbclient::K_NUM_7 => (KeyCode::Numpad7, None),
        orbclient::K_NUM_8 => (KeyCode::Numpad8, None),
        orbclient::K_NUM_9 => (KeyCode::Numpad9, None),
        orbclient::K_PERIOD => (KeyCode::Period, None),
        orbclient::K_PGDN => (KeyCode::PageDown, Some(NamedKey::PageDown)),
        orbclient::K_PGUP => (KeyCode::PageUp, Some(NamedKey::PageUp)),
        orbclient::K_QUOTE => (KeyCode::Quote, None),
        orbclient::K_RIGHT => (KeyCode::ArrowRight, Some(NamedKey::ArrowRight)),
        orbclient::K_RIGHT_SHIFT => (KeyCode::ShiftRight, Some(NamedKey::Shift)),
        orbclient::K_SEMICOLON => (KeyCode::Semicolon, None),
        orbclient::K_SLASH => (KeyCode::Slash, None),
        orbclient::K_SPACE => (KeyCode::Space, Some(NamedKey::Space)),
        orbclient::K_SUPER => (KeyCode::SuperLeft, Some(NamedKey::Super)),
        orbclient::K_TAB => (KeyCode::Tab, Some(NamedKey::Tab)),
        orbclient::K_TICK => (KeyCode::Backquote, None),
        orbclient::K_UP => (KeyCode::ArrowUp, Some(NamedKey::ArrowUp)),
        orbclient::K_VOLUME_DOWN => {
            (KeyCode::AudioVolumeDown, Some(NamedKey::AudioVolumeDown))
        }
        orbclient::K_VOLUME_TOGGLE => {
            (KeyCode::AudioVolumeMute, Some(NamedKey::AudioVolumeMute))
        }
        orbclient::K_VOLUME_UP => (KeyCode::AudioVolumeUp, Some(NamedKey::AudioVolumeUp)),

        _ => return (PhysicalKey::Unidentified(NativeKeyCode::Unidentified), None),
    };
    (PhysicalKey::Code(key_code), named_key_opt)
}

fn element_state(pressed: bool) -> event::ElementState {
    if pressed {
        event::ElementState::Pressed
    } else {
        event::ElementState::Released
    }
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct KeyboardModifierState: u8 {
        const LSHIFT = 1 << 0;
        const RSHIFT = 1 << 1;
        const LCTRL = 1 << 2;
        const RCTRL = 1 << 3;
        const LALT = 1 << 4;
        const RALT = 1 << 5;
        const LSUPER = 1 << 6;
        const RSUPER = 1 << 7;
    }
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct MouseButtonState: u8 {
        const LEFT = 1 << 0;
        const MIDDLE = 1 << 1;
        const RIGHT = 1 << 2;
    }
}

#[derive(Default)]
struct EventState {
    keyboard: KeyboardModifierState,
    mouse: MouseButtonState,
    resize_opt: Option<(u32, u32)>,
}

impl EventState {
    fn character_all_modifiers(&self, character: char) -> char {
        // Modify character if Ctrl is pressed
        #[allow(clippy::collapsible_if)]
        if self.keyboard.contains(KeyboardModifierState::LCTRL)
            || self.keyboard.contains(KeyboardModifierState::RCTRL)
        {
            if character.is_ascii_lowercase() {
                return ((character as u8 - b'a') + 1) as char;
            }
            // TODO: more control key variants?
        }

        // Return character as-is if no special handling required
        character
    }

    fn key(&mut self, key: PhysicalKey, pressed: bool) {
        let code = match key {
            PhysicalKey::Code(code) => code,
            _ => return,
        };

        match code {
            KeyCode::ShiftLeft => {
                self.keyboard.set(KeyboardModifierState::LSHIFT, pressed)
            }
            KeyCode::ShiftRight => {
                self.keyboard.set(KeyboardModifierState::RSHIFT, pressed)
            }
            KeyCode::ControlLeft => {
                self.keyboard.set(KeyboardModifierState::LCTRL, pressed)
            }
            KeyCode::ControlRight => {
                self.keyboard.set(KeyboardModifierState::RCTRL, pressed)
            }
            KeyCode::AltLeft => self.keyboard.set(KeyboardModifierState::LALT, pressed),
            KeyCode::AltRight => self.keyboard.set(KeyboardModifierState::RALT, pressed),
            KeyCode::SuperLeft => {
                self.keyboard.set(KeyboardModifierState::LSUPER, pressed)
            }
            KeyCode::SuperRight => {
                self.keyboard.set(KeyboardModifierState::RSUPER, pressed)
            }
            _ => (),
        }
    }

    fn mouse(
        &mut self,
        left: bool,
        middle: bool,
        right: bool,
    ) -> Option<(event::MouseButton, event::ElementState)> {
        if self.mouse.contains(MouseButtonState::LEFT) != left {
            self.mouse.set(MouseButtonState::LEFT, left);
            return Some((event::MouseButton::Left, element_state(left)));
        }

        if self.mouse.contains(MouseButtonState::MIDDLE) != middle {
            self.mouse.set(MouseButtonState::MIDDLE, middle);
            return Some((event::MouseButton::Middle, element_state(middle)));
        }

        if self.mouse.contains(MouseButtonState::RIGHT) != right {
            self.mouse.set(MouseButtonState::RIGHT, right);
            return Some((event::MouseButton::Right, element_state(right)));
        }

        None
    }

    fn modifiers(&self) -> Modifiers {
        let mut state = ModifiersState::empty();
        let mut pressed_mods = ModifiersKeys::empty();

        if self
            .keyboard
            .intersects(KeyboardModifierState::LSHIFT | KeyboardModifierState::RSHIFT)
        {
            state |= ModifiersState::SHIFT;
        }

        pressed_mods.set(
            ModifiersKeys::LSHIFT,
            self.keyboard.contains(KeyboardModifierState::LSHIFT),
        );
        pressed_mods.set(
            ModifiersKeys::RSHIFT,
            self.keyboard.contains(KeyboardModifierState::RSHIFT),
        );

        if self
            .keyboard
            .intersects(KeyboardModifierState::LCTRL | KeyboardModifierState::RCTRL)
        {
            state |= ModifiersState::CONTROL;
        }

        pressed_mods.set(
            ModifiersKeys::LCONTROL,
            self.keyboard.contains(KeyboardModifierState::LCTRL),
        );
        pressed_mods.set(
            ModifiersKeys::RCONTROL,
            self.keyboard.contains(KeyboardModifierState::RCTRL),
        );

        if self
            .keyboard
            .intersects(KeyboardModifierState::LALT | KeyboardModifierState::RALT)
        {
            state |= ModifiersState::ALT;
        }

        pressed_mods.set(
            ModifiersKeys::LALT,
            self.keyboard.contains(KeyboardModifierState::LALT),
        );
        pressed_mods.set(
            ModifiersKeys::RALT,
            self.keyboard.contains(KeyboardModifierState::RALT),
        );

        if self
            .keyboard
            .intersects(KeyboardModifierState::LSUPER | KeyboardModifierState::RSUPER)
        {
            state |= ModifiersState::SUPER
        }

        pressed_mods.set(
            ModifiersKeys::LSUPER,
            self.keyboard.contains(KeyboardModifierState::LSUPER),
        );
        pressed_mods.set(
            ModifiersKeys::RSUPER,
            self.keyboard.contains(KeyboardModifierState::RSUPER),
        );

        Modifiers {
            state,
            pressed_mods,
        }
    }
}

pub struct EventLoop<T> {
    windows: Vec<(Arc<RedoxSocket>, EventState)>,
    window_target: event_loop::ActiveEventLoop,
    user_events_sender: mpsc::Sender<T>,
    user_events_receiver: mpsc::Receiver<T>,
}

impl<T: 'static> EventLoop<T> {
    pub(crate) fn new(
        _: &PlatformSpecificEventLoopAttributes,
    ) -> Result<Self, EventLoopError> {
        let (user_events_sender, user_events_receiver) = mpsc::channel();

        let event_socket = Arc::new(
            RedoxSocket::event()
                .map_err(OsError::new)
                .map_err(|error| EventLoopError::Os(os_error!(error)))?,
        );

        let wake_socket = Arc::new(
            TimeSocket::open()
                .map_err(OsError::new)
                .map_err(|error| EventLoopError::Os(os_error!(error)))?,
        );

        event_socket
            .write(&syscall::Event {
                id: wake_socket.0.fd,
                flags: syscall::EventFlags::EVENT_READ,
                data: wake_socket.0.fd,
            })
            .map_err(OsError::new)
            .map_err(|error| EventLoopError::Os(os_error!(error)))?;

        Ok(Self {
            windows: Vec::new(),
            window_target: event_loop::ActiveEventLoop {
                p: ActiveEventLoop {
                    control_flow: Cell::new(ControlFlow::default()),
                    exit: Cell::new(false),
                    creates: Mutex::new(VecDeque::new()),
                    redraws: Arc::new(Mutex::new(VecDeque::new())),
                    destroys: Arc::new(Mutex::new(VecDeque::new())),
                    event_socket,
                    wake_socket,
                },
                _marker: PhantomData,
            },
            user_events_sender,
            user_events_receiver,
        })
    }

    fn process_event<F>(
        window_id: WindowId,
        event_option: EventOption,
        event_state: &mut EventState,
        mut event_handler: F,
    ) where
        F: FnMut(event::Event<T>),
    {
        match event_option {
            EventOption::Key(KeyEvent {
                character,
                scancode,
                pressed,
            }) => {
                // Convert scancode
                let (physical_key, named_key_opt) = convert_scancode(scancode);

                // Get previous modifiers and update modifiers based on physical key
                let modifiers_before = event_state.keyboard;
                event_state.key(physical_key, pressed);

                // Default to unidentified key with no text
                let mut logical_key = Key::Unidentified(NativeKey::Unidentified);
                let mut key_without_modifiers = logical_key.clone();
                let mut text = None;
                let mut text_with_all_modifiers = None;

                // Set key and text based on character
                if character != '\0' {
                    let mut tmp = [0u8; 4];
                    let character_str = character.encode_utf8(&mut tmp);
                    // The key with Shift and Caps Lock applied (but not Ctrl)
                    logical_key = Key::Character(character_str.into());
                    // The key without Shift or Caps Lock applied
                    key_without_modifiers =
                        Key::Character(SmolStr::from_iter(character.to_lowercase()));
                    if pressed {
                        // The key with Shift and Caps Lock applied (but not Ctrl)
                        text = Some(character_str.into());
                        // The key with Shift, Caps Lock, and Ctrl applied
                        let character_all_modifiers =
                            event_state.character_all_modifiers(character);
                        text_with_all_modifiers =
                            Some(character_all_modifiers.encode_utf8(&mut tmp).into())
                    }
                };

                // Override key if a named key was found (this is to allow Enter to replace '\n')
                if let Some(named_key) = named_key_opt {
                    logical_key = Key::Named(named_key);
                    key_without_modifiers = logical_key.clone();
                }

                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::KeyboardInput {
                        device_id: event::DeviceId(DeviceId),
                        event: event::KeyEvent {
                            logical_key,
                            physical_key,
                            location: KeyLocation::Standard,
                            state: element_state(pressed),
                            repeat: false,
                            text,
                            platform_specific: KeyEventExtra {
                                key_without_modifiers,
                                text_with_all_modifiers,
                            },
                        },
                        is_synthetic: false,
                    },
                });

                // If the state of the modifiers has changed, send the event.
                if modifiers_before != event_state.keyboard {
                    event_handler(event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::ModifiersChanged(
                            event_state.modifiers(),
                        ),
                    })
                }
            }
            EventOption::TextInput(TextInputEvent { character }) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::Ime(Ime::Preedit("".into(), None)),
                });
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::Ime(Ime::Commit(character.into())),
                });
            }
            EventOption::Mouse(MouseEvent { x, y }) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::CursorMoved {
                        device_id: event::DeviceId(DeviceId),
                        position: (x, y).into(),
                    },
                });
            }
            EventOption::MouseRelative(MouseRelativeEvent { dx, dy }) => {
                event_handler(event::Event::DeviceEvent {
                    device_id: event::DeviceId(DeviceId),
                    event: event::DeviceEvent::MouseMotion {
                        delta: (dx as f64, dy as f64),
                    },
                });
            }
            EventOption::Button(ButtonEvent {
                left,
                middle,
                right,
            }) => {
                while let Some((button, state)) = event_state.mouse(left, middle, right) {
                    event_handler(event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::MouseInput {
                            device_id: event::DeviceId(DeviceId),
                            state,
                            button,
                        },
                    });
                }
            }
            EventOption::Scroll(ScrollEvent { x, y }) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::MouseWheel {
                        device_id: event::DeviceId(DeviceId),
                        delta: event::MouseScrollDelta::LineDelta(x as f32, y as f32),
                        phase: event::TouchPhase::Moved,
                    },
                });
            }
            EventOption::Quit(QuitEvent {}) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::CloseRequested,
                });
            }
            EventOption::Focus(FocusEvent { focused }) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::Focused(focused),
                });
            }
            EventOption::Move(MoveEvent { x, y }) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::Moved((x, y).into()),
                });
            }
            EventOption::Resize(ResizeEvent { width, height }) => {
                event_handler(event::Event::WindowEvent {
                    window_id: RootWindowId(window_id),
                    event: event::WindowEvent::Resized((width, height).into()),
                });

                // Acknowledge resize after event loop.
                event_state.resize_opt = Some((width, height));
            }
            // TODO: Screen, Clipboard, Drop
            EventOption::Hover(HoverEvent { entered }) => {
                if entered {
                    event_handler(event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::CursorEntered {
                            device_id: event::DeviceId(DeviceId),
                        },
                    });
                } else {
                    event_handler(event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::CursorLeft {
                            device_id: event::DeviceId(DeviceId),
                        },
                    });
                }
            }
            other => {
                tracing::warn!("unhandled event: {:?}", other);
            }
        }
    }

    pub fn run<F>(mut self, mut event_handler_inner: F) -> Result<(), EventLoopError>
    where
        F: FnMut(event::Event<T>, &event_loop::ActiveEventLoop),
    {
        let mut event_handler =
            move |event: event::Event<T>, window_target: &event_loop::ActiveEventLoop| {
                event_handler_inner(event, window_target);
            };

        let mut start_cause = StartCause::Init;

        loop {
            event_handler(event::Event::NewEvents(start_cause), &self.window_target);

            if start_cause == StartCause::Init {
                event_handler(event::Event::Resumed, &self.window_target);
            }

            // Handle window creates.
            while let Some(window) = {
                let mut creates = self.window_target.p.creates.lock().unwrap();
                creates.pop_front()
            } {
                let window_id = WindowId {
                    fd: window.fd as u64,
                };

                let mut buf: [u8; 4096] = [0; 4096];
                let path = window.fpath(&mut buf).expect("failed to read properties");
                let properties = WindowProperties::new(path);

                self.windows.push((window, EventState::default()));

                // Send resize event on create to indicate first size.
                event_handler(
                    event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::Resized(
                            (properties.w, properties.h).into(),
                        ),
                    },
                    &self.window_target,
                );

                // Send resize event on create to indicate first position.
                event_handler(
                    event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::Moved(
                            (properties.x, properties.y).into(),
                        ),
                    },
                    &self.window_target,
                );
            }

            // Handle window destroys.
            while let Some(destroy_id) = {
                let mut destroys = self.window_target.p.destroys.lock().unwrap();
                destroys.pop_front()
            } {
                event_handler(
                    event::Event::WindowEvent {
                        window_id: RootWindowId(destroy_id),
                        event: event::WindowEvent::Destroyed,
                    },
                    &self.window_target,
                );

                self.windows
                    .retain(|(window, _event_state)| window.fd as u64 != destroy_id.fd);
            }

            // Handle window events.
            let mut i = 0;
            // While loop is used here because the same window may be processed more than once.
            while let Some((window, event_state)) = self.windows.get_mut(i) {
                let window_id = WindowId {
                    fd: window.fd as u64,
                };

                let mut event_buf = [0u8; 16 * mem::size_of::<orbclient::Event>()];
                let count = syscall::read(window.fd, &mut event_buf)
                    .expect("failed to read window events");
                // Safety: orbclient::Event is a packed struct designed to be transferred over a
                // socket.
                let events = unsafe {
                    slice::from_raw_parts(
                        event_buf.as_ptr() as *const orbclient::Event,
                        count / mem::size_of::<orbclient::Event>(),
                    )
                };

                for orbital_event in events {
                    Self::process_event(
                        window_id,
                        orbital_event.to_option(),
                        event_state,
                        |event| event_handler(event, &self.window_target),
                    );
                }

                if count == event_buf.len() {
                    // If event buf was full, process same window again to ensure all events are
                    // drained.
                    continue;
                }

                // Acknowledge the latest resize event.
                if let Some((w, h)) = event_state.resize_opt.take() {
                    window
                        .write(format!("S,{w},{h}").as_bytes())
                        .expect("failed to acknowledge resize");

                    // Require redraw after resize.
                    let mut redraws = self.window_target.p.redraws.lock().unwrap();
                    if !redraws.contains(&window_id) {
                        redraws.push_back(window_id);
                    }
                }

                // Move to next window.
                i += 1;
            }

            while let Ok(event) = self.user_events_receiver.try_recv() {
                event_handler(event::Event::UserEvent(event), &self.window_target);
            }

            // To avoid deadlocks the redraws lock is not held during event processing.
            while let Some(window_id) = {
                let mut redraws = self.window_target.p.redraws.lock().unwrap();
                redraws.pop_front()
            } {
                event_handler(
                    event::Event::WindowEvent {
                        window_id: RootWindowId(window_id),
                        event: event::WindowEvent::RedrawRequested,
                    },
                    &self.window_target,
                );
            }

            event_handler(event::Event::AboutToWait, &self.window_target);

            if self.window_target.p.exiting() {
                break;
            }

            let requested_resume = match self.window_target.p.control_flow() {
                ControlFlow::Poll => {
                    start_cause = StartCause::Poll;
                    continue;
                }
                ControlFlow::Wait => None,
                ControlFlow::WaitUntil(instant) => Some(instant),
            };

            // Re-using wake socket caused extra wake events before because there were leftover
            // timeouts, and then new timeouts were added each time a spurious timeout expired.
            let timeout_socket = TimeSocket::open().unwrap();

            self.window_target
                .p
                .event_socket
                .write(&syscall::Event {
                    id: timeout_socket.0.fd,
                    flags: syscall::EventFlags::EVENT_READ,
                    data: 0,
                })
                .unwrap();

            let start = Instant::now();
            if let Some(instant) = requested_resume {
                let mut time = timeout_socket.current_time().unwrap();

                if let Some(duration) = instant.checked_duration_since(start) {
                    time.tv_sec += duration.as_secs() as i64;
                    time.tv_nsec += duration.subsec_nanos() as i32;
                    // Normalize timespec so tv_nsec is not greater than one second.
                    while time.tv_nsec >= 1_000_000_000 {
                        time.tv_sec += 1;
                        time.tv_nsec -= 1_000_000_000;
                    }
                }

                timeout_socket.timeout(&time).unwrap();
            }

            // Wait for event if needed.
            let mut event = syscall::Event::default();
            self.window_target.p.event_socket.read(&mut event).unwrap();

            // TODO: handle spurious wakeups (redraw caused wakeup but redraw already handled)
            match requested_resume {
                Some(requested_resume) if event.id == timeout_socket.0.fd => {
                    // If the event is from the special timeout socket, report that resume
                    // time was reached.
                    start_cause = StartCause::ResumeTimeReached {
                        start,
                        requested_resume,
                    };
                }
                _ => {
                    // Normal window event or spurious timeout.
                    start_cause = StartCause::WaitCancelled {
                        start,
                        requested_resume,
                    };
                }
            }
        }

        event_handler(event::Event::LoopExiting, &self.window_target);

        Ok(())
    }

    pub fn window_target(&self) -> &event_loop::ActiveEventLoop {
        &self.window_target
    }

    pub fn create_proxy(&self) -> EventLoopProxy<T> {
        EventLoopProxy {
            user_events_sender: self.user_events_sender.clone(),
            wake_socket: self.window_target.p.wake_socket.clone(),
        }
    }
}

pub struct EventLoopProxy<T: 'static> {
    user_events_sender: mpsc::Sender<T>,
    wake_socket: Arc<TimeSocket>,
}

impl<T> EventLoopProxy<T> {
    pub fn send_event(&self, event: T) -> Result<(), event_loop::EventLoopClosed<T>> {
        self.user_events_sender
            .send(event)
            .map_err(|mpsc::SendError(x)| event_loop::EventLoopClosed(x))?;

        self.wake_socket.wake().unwrap();

        Ok(())
    }
}

impl<T> Clone for EventLoopProxy<T> {
    fn clone(&self) -> Self {
        Self {
            user_events_sender: self.user_events_sender.clone(),
            wake_socket: self.wake_socket.clone(),
        }
    }
}

impl<T> Unpin for EventLoopProxy<T> {}

pub struct ActiveEventLoop {
    control_flow: Cell<ControlFlow>,
    exit: Cell<bool>,
    pub(super) creates: Mutex<VecDeque<Arc<RedoxSocket>>>,
    pub(super) redraws: Arc<Mutex<VecDeque<WindowId>>>,
    pub(super) destroys: Arc<Mutex<VecDeque<WindowId>>>,
    pub(super) event_socket: Arc<RedoxSocket>,
    pub(super) wake_socket: Arc<TimeSocket>,
}

impl ActiveEventLoop {
    pub fn create_custom_cursor(&self, source: CustomCursorSource) -> RootCustomCursor {
        let _ = source.inner;
        RootCustomCursor {
            inner: super::PlatformCustomCursor,
        }
    }

    pub fn primary_monitor(&self) -> Option<MonitorHandle> {
        Some(MonitorHandle)
    }

    pub fn available_monitors(&self) -> VecDeque<MonitorHandle> {
        let mut v = VecDeque::with_capacity(1);
        v.push_back(MonitorHandle);
        v
    }

    #[inline]
    pub fn listen_device_events(&self, _allowed: DeviceEvents) {}

    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::RawDisplayHandle::Orbital(
            raw_window_handle::OrbitalDisplayHandle::new(),
        ))
    }

    pub fn set_control_flow(&self, control_flow: ControlFlow) {
        self.control_flow.set(control_flow)
    }

    pub fn control_flow(&self) -> ControlFlow {
        self.control_flow.get()
    }

    pub(crate) fn exit(&self) {
        self.exit.set(true);
    }

    pub(crate) fn exiting(&self) -> bool {
        self.exit.get()
    }

    pub(crate) fn owned_display_handle(&self) -> OwnedDisplayHandle {
        OwnedDisplayHandle
    }
}

#[derive(Clone)]
pub(crate) struct OwnedDisplayHandle;

impl OwnedDisplayHandle {
    #[inline]
    pub fn raw_display_handle_raw_window_handle(
        &self,
    ) -> Result<raw_window_handle::RawDisplayHandle, raw_window_handle::HandleError> {
        Ok(raw_window_handle::OrbitalDisplayHandle::new().into())
    }
}
