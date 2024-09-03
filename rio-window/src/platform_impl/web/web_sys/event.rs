use crate::dpi::LogicalPosition;
use crate::event::{MouseButton, MouseScrollDelta};
use crate::keyboard::{Key, KeyLocation, ModifiersState, NamedKey, PhysicalKey};

use smol_str::SmolStr;
use std::cell::OnceCell;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{KeyboardEvent, MouseEvent, PointerEvent, WheelEvent};

bitflags::bitflags! {
    // https://www.w3.org/TR/pointerevents3/#the-buttons-property
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ButtonsState: u16 {
        const LEFT    = 0b00001;
        const RIGHT   = 0b00010;
        const MIDDLE  = 0b00100;
        const BACK    = 0b01000;
        const FORWARD = 0b10000;
    }
}

impl From<ButtonsState> for MouseButton {
    fn from(value: ButtonsState) -> Self {
        match value {
            ButtonsState::LEFT => MouseButton::Left,
            ButtonsState::RIGHT => MouseButton::Right,
            ButtonsState::MIDDLE => MouseButton::Middle,
            ButtonsState::BACK => MouseButton::Back,
            ButtonsState::FORWARD => MouseButton::Forward,
            _ => MouseButton::Other(value.bits()),
        }
    }
}

impl From<MouseButton> for ButtonsState {
    fn from(value: MouseButton) -> Self {
        match value {
            MouseButton::Left => ButtonsState::LEFT,
            MouseButton::Right => ButtonsState::RIGHT,
            MouseButton::Middle => ButtonsState::MIDDLE,
            MouseButton::Back => ButtonsState::BACK,
            MouseButton::Forward => ButtonsState::FORWARD,
            MouseButton::Other(value) => ButtonsState::from_bits_retain(value),
        }
    }
}

pub fn mouse_buttons(event: &MouseEvent) -> ButtonsState {
    ButtonsState::from_bits_retain(event.buttons())
}

pub fn mouse_button(event: &MouseEvent) -> Option<MouseButton> {
    // https://www.w3.org/TR/pointerevents3/#the-button-property
    match event.button() {
        -1 => None,
        0 => Some(MouseButton::Left),
        1 => Some(MouseButton::Middle),
        2 => Some(MouseButton::Right),
        3 => Some(MouseButton::Back),
        4 => Some(MouseButton::Forward),
        i => Some(MouseButton::Other(
            i.try_into()
                .expect("unexpected negative mouse button value"),
        )),
    }
}

impl MouseButton {
    pub fn to_id(self) -> u32 {
        match self {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(value) => value.into(),
        }
    }
}

pub fn mouse_position(event: &MouseEvent) -> LogicalPosition<f64> {
    #[wasm_bindgen]
    extern "C" {
        type MouseEventExt;

        #[wasm_bindgen(method, getter, js_name = offsetX)]
        fn offset_x(this: &MouseEventExt) -> f64;

        #[wasm_bindgen(method, getter, js_name = offsetY)]
        fn offset_y(this: &MouseEventExt) -> f64;
    }

    let event: &MouseEventExt = event.unchecked_ref();

    LogicalPosition {
        x: event.offset_x(),
        y: event.offset_y(),
    }
}

// TODO: Remove this when Firefox supports correct movement values in coalesced events.
// See <https://bugzilla.mozilla.org/show_bug.cgi?id=1753724>.
pub struct MouseDelta(Option<MouseDeltaInner>);

pub struct MouseDeltaInner {
    old_position: LogicalPosition<f64>,
    old_delta: LogicalPosition<f64>,
}

impl MouseDelta {
    pub fn init(window: &web_sys::Window, event: &PointerEvent) -> Self {
        // Firefox has wrong movement values in coalesced events, we will detect that by checking
        // for `pointerrawupdate` support. Presumably an implementation of `pointerrawupdate`
        // should require correct movement values, otherwise uncoalesced events might be broken as
        // well.
        Self(
            (!has_pointer_raw_support(window) && has_coalesced_events_support(event))
                .then(|| MouseDeltaInner {
                    old_position: mouse_position(event),
                    old_delta: LogicalPosition {
                        x: event.movement_x() as f64,
                        y: event.movement_y() as f64,
                    },
                }),
        )
    }

    pub fn delta(&mut self, event: &MouseEvent) -> LogicalPosition<f64> {
        if let Some(inner) = &mut self.0 {
            let new_position = mouse_position(event);
            let x = new_position.x - inner.old_position.x + inner.old_delta.x;
            let y = new_position.y - inner.old_position.y + inner.old_delta.y;
            inner.old_position = new_position;
            inner.old_delta = LogicalPosition::new(0., 0.);
            LogicalPosition::new(x, y)
        } else {
            LogicalPosition {
                x: event.movement_x() as f64,
                y: event.movement_y() as f64,
            }
        }
    }
}

pub fn mouse_scroll_delta(
    window: &web_sys::Window,
    event: &WheelEvent,
) -> Option<MouseScrollDelta> {
    let x = -event.delta_x();
    let y = -event.delta_y();

    match event.delta_mode() {
        WheelEvent::DOM_DELTA_LINE => {
            Some(MouseScrollDelta::LineDelta(x as f32, y as f32))
        }
        WheelEvent::DOM_DELTA_PIXEL => {
            let delta =
                LogicalPosition::new(x, y).to_physical(super::scale_factor(window));
            Some(MouseScrollDelta::PixelDelta(delta))
        }
        _ => None,
    }
}

pub fn key_code(event: &KeyboardEvent) -> PhysicalKey {
    let code = event.code();
    PhysicalKey::from_key_code_attribute_value(&code)
}

pub fn key(event: &KeyboardEvent) -> Key {
    Key::from_key_attribute_value(&event.key())
}

pub fn key_text(event: &KeyboardEvent) -> Option<SmolStr> {
    let key = event.key();
    let key = Key::from_key_attribute_value(&key);
    match &key {
        Key::Character(text) => Some(text.clone()),
        Key::Named(NamedKey::Tab) => Some(SmolStr::new("\t")),
        Key::Named(NamedKey::Enter) => Some(SmolStr::new("\r")),
        Key::Named(NamedKey::Space) => Some(SmolStr::new(" ")),
        _ => None,
    }
    .map(SmolStr::new)
}

pub fn key_location(event: &KeyboardEvent) -> KeyLocation {
    match event.location() {
        KeyboardEvent::DOM_KEY_LOCATION_LEFT => KeyLocation::Left,
        KeyboardEvent::DOM_KEY_LOCATION_RIGHT => KeyLocation::Right,
        KeyboardEvent::DOM_KEY_LOCATION_NUMPAD => KeyLocation::Numpad,
        KeyboardEvent::DOM_KEY_LOCATION_STANDARD => KeyLocation::Standard,
        location => {
            tracing::warn!("Unexpected key location: {location}");
            KeyLocation::Standard
        }
    }
}

pub fn keyboard_modifiers(event: &KeyboardEvent) -> ModifiersState {
    let mut state = ModifiersState::empty();

    if event.shift_key() {
        state |= ModifiersState::SHIFT;
    }
    if event.ctrl_key() {
        state |= ModifiersState::CONTROL;
    }
    if event.alt_key() {
        state |= ModifiersState::ALT;
    }
    if event.meta_key() {
        state |= ModifiersState::SUPER;
    }

    state
}

pub fn mouse_modifiers(event: &MouseEvent) -> ModifiersState {
    let mut state = ModifiersState::empty();

    if event.shift_key() {
        state |= ModifiersState::SHIFT;
    }
    if event.ctrl_key() {
        state |= ModifiersState::CONTROL;
    }
    if event.alt_key() {
        state |= ModifiersState::ALT;
    }
    if event.meta_key() {
        state |= ModifiersState::SUPER;
    }

    state
}

pub fn pointer_move_event(event: PointerEvent) -> impl Iterator<Item = PointerEvent> {
    // make a single iterator depending on the availability of coalesced events
    if has_coalesced_events_support(&event) {
        None.into_iter().chain(
            Some(
                event
                    .get_coalesced_events()
                    .into_iter()
                    .map(PointerEvent::unchecked_from_js),
            )
            .into_iter()
            .flatten(),
        )
    } else {
        Some(event).into_iter().chain(None.into_iter().flatten())
    }
}

// TODO: Remove when all browsers implement it correctly.
// See <https://github.com/rust-windowing/winit/issues/2875>.
pub fn has_pointer_raw_support(window: &web_sys::Window) -> bool {
    thread_local! {
        static POINTER_RAW_SUPPORT: OnceCell<bool> = const { OnceCell::new() };
    }

    POINTER_RAW_SUPPORT.with(|support| {
        *support.get_or_init(|| {
            #[wasm_bindgen]
            extern "C" {
                type PointerRawSupport;

                #[wasm_bindgen(method, getter, js_name = onpointerrawupdate)]
                fn has_on_pointerrawupdate(this: &PointerRawSupport) -> JsValue;
            }

            let support: &PointerRawSupport = window.unchecked_ref();
            !support.has_on_pointerrawupdate().is_undefined()
        })
    })
}

// TODO: Remove when Safari supports `getCoalescedEvents`.
// See <https://bugs.webkit.org/show_bug.cgi?id=210454>.
pub fn has_coalesced_events_support(event: &PointerEvent) -> bool {
    thread_local! {
        static COALESCED_EVENTS_SUPPORT: OnceCell<bool> = const { OnceCell::new() };
    }

    COALESCED_EVENTS_SUPPORT.with(|support| {
        *support.get_or_init(|| {
            #[wasm_bindgen]
            extern "C" {
                type PointerCoalescedEventsSupport;

                #[wasm_bindgen(method, getter, js_name = getCoalescedEvents)]
                fn has_get_coalesced_events(
                    this: &PointerCoalescedEventsSupport,
                ) -> JsValue;
            }

            let support: &PointerCoalescedEventsSupport = event.unchecked_ref();
            !support.has_get_coalesced_events().is_undefined()
        })
    })
}
