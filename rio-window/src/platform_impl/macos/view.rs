#![allow(clippy::unnecessary_cast)]
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ptr;

use objc2::rc::{Retained, WeakId};
use objc2::runtime::{AnyObject, Sel};
use objc2::{declare_class, msg_send_id, mutability, sel, ClassType, DeclaredClass};
use objc2_app_kit::{
    NSApplication, NSCursor, NSEvent, NSEventPhase, NSResponder, NSTextInputClient,
    NSTrackingRectTag, NSView, NSViewFrameDidChangeNotification,
};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSAttributedString, NSAttributedStringKey, NSCopying,
    NSMutableAttributedString, NSNotFound, NSNotificationCenter, NSObject,
    NSObjectProtocol, NSPoint, NSRange, NSRect, NSSize, NSString, NSUInteger,
};

use super::app_delegate::ApplicationDelegate;
use super::cursor::{default_cursor, invisible_cursor};
use super::event::{
    code_to_key, code_to_location, create_key_event, event_mods, lalt_pressed,
    ralt_pressed, scancode_to_physicalkey,
};
use super::window::WinitWindow;
use super::window_delegate::WindowDelegate;
use super::DEVICE_ID;
use crate::dpi::{LogicalPosition, LogicalSize};
use crate::event::{
    DeviceEvent, ElementState, Ime, Modifiers, MouseButton, MouseScrollDelta, TouchPhase,
    WindowEvent,
};
use crate::keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey};
use crate::platform::macos::OptionAsAlt;

#[derive(Debug)]
struct CursorState {
    visible: bool,
    cursor: Retained<NSCursor>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            visible: true,
            cursor: default_cursor(),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Default)]
enum ImeState {
    #[default]
    /// The IME events are disabled, so only `ReceivedCharacter` is being sent to the user.
    Disabled,

    /// The ground state of enabled IME input. It means that both Preedit and regular keyboard
    /// input could be start from it.
    Ground,

    /// The IME is in preedit.
    Preedit,

    /// The text was just committed, so the next input from the keyboard must be ignored.
    Committed,
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct ModLocationMask: u8 {
        const LEFT     = 0b0001;
        const RIGHT    = 0b0010;
    }
}
impl ModLocationMask {
    fn from_location(loc: KeyLocation) -> ModLocationMask {
        match loc {
            KeyLocation::Left => ModLocationMask::LEFT,
            KeyLocation::Right => ModLocationMask::RIGHT,
            _ => unreachable!(),
        }
    }
}

fn key_to_modifier(key: &Key) -> Option<ModifiersState> {
    match key {
        Key::Named(NamedKey::Alt) => Some(ModifiersState::ALT),
        Key::Named(NamedKey::Control) => Some(ModifiersState::CONTROL),
        Key::Named(NamedKey::Super) => Some(ModifiersState::SUPER),
        Key::Named(NamedKey::Shift) => Some(ModifiersState::SHIFT),
        _ => None,
    }
}

fn get_right_modifier_code(key: &Key) -> KeyCode {
    match key {
        Key::Named(NamedKey::Alt) => KeyCode::AltRight,
        Key::Named(NamedKey::Control) => KeyCode::ControlRight,
        Key::Named(NamedKey::Shift) => KeyCode::ShiftRight,
        Key::Named(NamedKey::Super) => KeyCode::SuperRight,
        _ => unreachable!(),
    }
}

fn get_left_modifier_code(key: &Key) -> KeyCode {
    match key {
        Key::Named(NamedKey::Alt) => KeyCode::AltLeft,
        Key::Named(NamedKey::Control) => KeyCode::ControlLeft,
        Key::Named(NamedKey::Shift) => KeyCode::ShiftLeft,
        Key::Named(NamedKey::Super) => KeyCode::SuperLeft,
        _ => unreachable!(),
    }
}

#[derive(Debug)]
pub struct ViewState {
    /// Strong reference to the global application state.
    app_delegate: Retained<ApplicationDelegate>,

    cursor_state: RefCell<CursorState>,
    ime_position: Cell<NSPoint>,
    ime_size: Cell<NSSize>,
    modifiers: Cell<Modifiers>,
    phys_modifiers: RefCell<HashMap<Key, ModLocationMask>>,
    tracking_rect: Cell<Option<NSTrackingRectTag>>,
    ime_state: Cell<ImeState>,
    input_source: RefCell<String>,

    /// True iff the application wants IME events.
    ///
    /// Can be set using `set_ime_allowed`
    ime_allowed: Cell<bool>,

    /// True if the current key event should be forwarded
    /// to the application, even during IME
    forward_key_to_app: Cell<bool>,

    /// True if we're currently processing a key event
    in_key_event: Cell<bool>,

    marked_text: RefCell<Retained<NSMutableAttributedString>>,
    accepts_first_mouse: bool,

    // Weak reference because the window keeps a strong reference to the view
    _ns_window: WeakId<WinitWindow>,

    /// The state of the `Option` as `Alt`.
    option_as_alt: Cell<OptionAsAlt>,
}

declare_class!(
    pub(super) struct WinitView;

    unsafe impl ClassType for WinitView {
        #[inherits(NSResponder, NSObject)]
        type Super = NSView;
        type Mutability = mutability::MainThreadOnly;
        const NAME: &'static str = "WinitView";
    }

    impl DeclaredClass for WinitView {
        type Ivars = ViewState;
    }

    unsafe impl WinitView {
        #[method(isFlipped)]
        fn is_flipped(&self) -> bool {
            // `winit` uses the upper-left corner as the origin.
            true
        }

        #[method(viewDidMoveToWindow)]
        fn view_did_move_to_window(&self) {
            trace_scope!("viewDidMoveToWindow");
            if let Some(tracking_rect) = self.ivars().tracking_rect.take() {
                self.removeTrackingRect(tracking_rect);
            }

            let rect = self.frame();
            let tracking_rect = unsafe {
                self.addTrackingRect_owner_userData_assumeInside(rect, self, ptr::null_mut(), false)
            };
            assert_ne!(tracking_rect, 0, "failed adding tracking rect");
            self.ivars().tracking_rect.set(Some(tracking_rect));
        }

        #[method(frameDidChange:)]
        fn frame_did_change(&self, _event: &NSEvent) {
            trace_scope!("frameDidChange:");
            if let Some(tracking_rect) = self.ivars().tracking_rect.take() {
                self.removeTrackingRect(tracking_rect);
            }

            let rect = self.frame();
            let tracking_rect = unsafe {
                self.addTrackingRect_owner_userData_assumeInside(rect, self, ptr::null_mut(), false)
            };
            assert_ne!(tracking_rect, 0, "failed adding tracking rect");
            self.ivars().tracking_rect.set(Some(tracking_rect));

            // Emit resize event here rather than from windowDidResize because:
            // 1. When a new window is created as a tab, the frame size may change without a window resize occurring.
            // 2. Even when a window resize does occur on a new tabbed window, it contains the wrong size (includes tab height).
            let logical_size = LogicalSize::new(rect.size.width as f64, rect.size.height as f64);
            let size = logical_size.to_physical::<u32>(self.scale_factor());
            self.queue_event(WindowEvent::Resized(size));
        }

        #[method(drawRect:)]
        fn draw_rect(&self, _rect: NSRect) {
            trace_scope!("drawRect:");

            // It's a workaround for https://github.com/rust-windowing/winit/issues/2640, don't replace with `self.window_id()`.
            if let Some(window) = self.ivars()._ns_window.load() {
                self.ivars().app_delegate.handle_redraw(window.id());
            }

            // This is a direct subclass of NSView, no need to call superclass' drawRect:
        }

        #[method(acceptsFirstResponder)]
        fn accepts_first_responder(&self) -> bool {
            trace_scope!("acceptsFirstResponder");
            true
        }

        // This is necessary to prevent a beefy terminal error on MacBook Pros:
        // IMKInputSession [0x7fc573576ff0 presentFunctionRowItemTextInputViewWithEndpoint:completionHandler:] : [self textInputContext]=0x7fc573558e10 *NO* NSRemoteViewController to client, NSError=Error Domain=NSCocoaErrorDomain Code=4099 "The connection from pid 0 was invalidated from this process." UserInfo={NSDebugDescription=The connection from pid 0 was invalidated from this process.}, com.apple.inputmethod.EmojiFunctionRowItem
        // TODO: Add an API extension for using `NSTouchBar`
        #[method_id(touchBar)]
        fn touch_bar(&self) -> Option<Retained<NSObject>> {
            trace_scope!("touchBar");
            None
        }

        #[method(resetCursorRects)]
        fn reset_cursor_rects(&self) {
            trace_scope!("resetCursorRects");
            let bounds = self.bounds();
            let cursor_state = self.ivars().cursor_state.borrow();
            // We correctly invoke `addCursorRect` only from inside `resetCursorRects`
            if cursor_state.visible {
                self.addCursorRect_cursor(bounds, &cursor_state.cursor);
            } else {
                self.addCursorRect_cursor(bounds, &invisible_cursor());
            }
        }
    }

    unsafe impl NSTextInputClient for WinitView {
        #[method(hasMarkedText)]
        fn has_marked_text(&self) -> bool {
            trace_scope!("hasMarkedText");
            self.ivars().marked_text.borrow().length() > 0
        }

        #[method(markedRange)]
        fn marked_range(&self) -> NSRange {
            trace_scope!("markedRange");
            let length = self.ivars().marked_text.borrow().length();
            if length > 0 {
                NSRange::new(0, length)
            } else {
                // Documented to return `{NSNotFound, 0}` if there is no marked range.
                NSRange::new(NSNotFound as NSUInteger, 0)
            }
        }

        #[method(selectedRange)]
        fn selected_range(&self) -> NSRange {
            trace_scope!("selectedRange");
            // Documented to return `{NSNotFound, 0}` if there is no selection.
            NSRange::new(NSNotFound as NSUInteger, 0)
        }

        #[method(setMarkedText:selectedRange:replacementRange:)]
        fn set_marked_text(
            &self,
            string: &NSObject,
            selected_range: NSRange,
            _replacement_range: NSRange,
        ) {
            trace_scope!("setMarkedText:selectedRange:replacementRange:");

            // SAFETY: This method is guaranteed to get either a `NSString` or a `NSAttributedString`.
            let (marked_text, string) = if string.is_kind_of::<NSAttributedString>() {
                let string: *const NSObject = string;
                let string: *const NSAttributedString = string.cast();
                let string = unsafe { &*string };
                (
                    NSMutableAttributedString::from_attributed_nsstring(string),
                    string.string(),
                )
            } else {
                let string: *const NSObject = string;
                let string: *const NSString = string.cast();
                let string = unsafe { &*string };
                (
                    NSMutableAttributedString::from_nsstring(string),
                    string.copy(),
                )
            };

            // Update marked text.
            *self.ivars().marked_text.borrow_mut() = marked_text;

            // Notify IME is active if application still doesn't know it.
            if self.ivars().ime_state.get() == ImeState::Disabled {
                *self.ivars().input_source.borrow_mut() = self.current_input_source();
                self.queue_event(WindowEvent::Ime(Ime::Enabled));
            }

            if unsafe { self.hasMarkedText() } {
                self.ivars().ime_state.set(ImeState::Preedit);
            } else {
                // In case the preedit was cleared, set IME into the Ground state.
                self.ivars().ime_state.set(ImeState::Ground);
            }

            let cursor_range = if string.is_empty() {
                // An empty string basically means that there's no preedit, so indicate that by
                // sending a `None` cursor range.
                None
            } else {
                // Convert the selected range from UTF-16 indices to UTF-8 indices.
                let sub_string_a = unsafe { string.substringToIndex(selected_range.location) };
                let sub_string_b = unsafe { string.substringToIndex(selected_range.end()) };
                let lowerbound_utf8 = sub_string_a.len();
                let upperbound_utf8 = sub_string_b.len();
                Some((lowerbound_utf8, upperbound_utf8))
            };

            // Send WindowEvent for updating marked text
            self.queue_event(WindowEvent::Ime(Ime::Preedit(string.to_string(), cursor_range)));
        }

        #[method(unmarkText)]
        fn unmark_text(&self) {
            trace_scope!("unmarkText");
            *self.ivars().marked_text.borrow_mut() = NSMutableAttributedString::new();

            // Only discard marked text if we have an input context
            if let Some(input_context) = self.inputContext() {
                input_context.discardMarkedText();
            }

            self.queue_event(WindowEvent::Ime(Ime::Preedit(String::new(), None)));
            if self.is_ime_enabled() {
                // Leave the Preedit state
                self.ivars().ime_state.set(ImeState::Ground);
            } else {
                tracing::warn!("Expected to have IME enabled when receiving unmarkText");
            }
        }

        #[method_id(validAttributesForMarkedText)]
        fn valid_attributes_for_marked_text(&self) -> Retained<NSArray<NSAttributedStringKey>> {
            trace_scope!("validAttributesForMarkedText");
            NSArray::new()
        }

        #[method_id(attributedSubstringForProposedRange:actualRange:)]
        fn attributed_substring_for_proposed_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> Option<Retained<NSAttributedString>> {
            trace_scope!("attributedSubstringForProposedRange:actualRange:");
            None
        }

        #[method(characterIndexForPoint:)]
        fn character_index_for_point(&self, _point: NSPoint) -> NSUInteger {
            trace_scope!("characterIndexForPoint:");
            0
        }

        #[method(firstRectForCharacterRange:actualRange:)]
        fn first_rect_for_character_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> NSRect {
            trace_scope!("firstRectForCharacterRange:actualRange:");

            let position = self.ivars().ime_position.get();
            let size = self.ivars().ime_size.get();

            // Validate coordinates before creating rect
            if position.x.is_nan() || position.y.is_nan() || size.width.is_nan() || size.height.is_nan() {
                tracing::warn!("Invalid IME coordinates in firstRectForCharacterRange");
                // Return a default rect at origin with minimal size
                let default_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1.0, 1.0));
                return self.window()
                    .convertRectToScreen(self.convertRect_toView(default_rect, None));
            }

            let rect = NSRect::new(position, size);

            // Return value is expected to be in screen coordinates, so we need a conversion here
            self.window()
                .convertRectToScreen(self.convertRect_toView(rect, None))
        }

        #[method(insertText:replacementRange:)]
        fn insert_text(&self, string: &NSObject, _replacement_range: NSRange) {
            trace_scope!("insertText:replacementRange:");

            // SAFETY: This method is guaranteed to get either a `NSString` or a `NSAttributedString`.
            let string = if string.is_kind_of::<NSAttributedString>() {
                let string: *const NSObject = string;
                let string: *const NSAttributedString = string.cast();
                unsafe { &*string }.string().to_string()
            } else {
                let string: *const NSObject = string;
                let string: *const NSString = string.cast();
                unsafe { &*string }.to_string()
            };

            let is_control = string.chars().next().is_some_and(|c| c.is_control());
            let has_marked_text = unsafe { self.hasMarkedText() };
            let is_in_key_event = self.ivars().in_key_event.get();

            if self.is_ime_enabled() {
                if has_marked_text && !is_control {
                    // Clear preedit and commit the text (normal IME flow)
                    self.queue_event(WindowEvent::Ime(Ime::Preedit(String::new(), None)));
                    self.queue_event(WindowEvent::Ime(Ime::Commit(string)));
                    self.ivars().ime_state.set(ImeState::Committed);
                } else if !is_control && !string.is_empty() && !is_in_key_event {
                    // Direct input not from keyboard (e.g., emoji picker)
                    self.queue_event(WindowEvent::Ime(Ime::Commit(string)));
                    self.ivars().ime_state.set(ImeState::Committed);
                }
            } else if !is_control && !string.is_empty() && !is_in_key_event {
                // IME is disabled but we got non-keyboard input (e.g., emoji picker)
                // Temporarily enable IME for this input
                self.queue_event(WindowEvent::Ime(Ime::Enabled));
                self.queue_event(WindowEvent::Ime(Ime::Commit(string)));
                self.ivars().ime_state.set(ImeState::Committed);
            }
        }

        // Basically, we're sent this message whenever a keyboard event that doesn't generate a "human
        // readable" character happens, i.e. newlines, tabs, and Ctrl+C.
        #[method(doCommandBySelector:)]
        fn do_command_by_selector(&self, _command: Sel) {
            trace_scope!("doCommandBySelector:");
            // We shouldn't forward any character from just committed text, since we'll end up sending
            // it twice with some IMEs like Korean one. We'll also always send `Enter` in that case,
            // which is not desired given it was used to confirm IME input.
            if self.ivars().ime_state.get() == ImeState::Committed {
                return;
            }

            self.ivars().forward_key_to_app.set(true);

            if unsafe { self.hasMarkedText() } && self.ivars().ime_state.get() == ImeState::Preedit
            {
                // Leave preedit so that we also report the key-up for this key.
                self.ivars().ime_state.set(ImeState::Ground);
            }
        }
    }

    unsafe impl WinitView {
        #[method(keyDown:)]
        fn key_down(&self, event: &NSEvent) {
            trace_scope!("keyDown:");

            // Set flag to indicate we're in a key event
            self.ivars().in_key_event.set(true);

            {
                let mut prev_input_source = self.ivars().input_source.borrow_mut();
                let current_input_source = self.current_input_source();
                if *prev_input_source != current_input_source && self.is_ime_enabled() {
                    *prev_input_source = current_input_source;
                    drop(prev_input_source);
                    self.ivars().ime_state.set(ImeState::Disabled);
                    self.queue_event(WindowEvent::Ime(Ime::Disabled));
                }
            }

            // Get the characters from the event.
            let old_ime_state = self.ivars().ime_state.get();
            self.ivars().forward_key_to_app.set(false);
            let event = replace_event(event, self.option_as_alt());

            // The `interpretKeyEvents` function might call
            // `setMarkedText`, `insertText`, and `doCommandBySelector`.
            // It's important that we call this before queuing the KeyboardInput, because
            // we must send the `KeyboardInput` event during IME if it triggered
            // `doCommandBySelector`. (doCommandBySelector means that the keyboard input
            // is not handled by IME and should be handled by the application)
            if self.ivars().ime_allowed.get() {
                let events_for_nsview = NSArray::from_slice(&[&*event]);
                unsafe { self.interpretKeyEvents(&events_for_nsview) };

                // If the text was committed we must treat the next keyboard event as IME related.
                if self.ivars().ime_state.get() == ImeState::Committed {
                    // Remove any marked text, so normal input can continue.
                    *self.ivars().marked_text.borrow_mut() = NSMutableAttributedString::new();
                }
            }

            self.update_modifiers(&event, false);

            let had_ime_input = match self.ivars().ime_state.get() {
                ImeState::Committed => {
                    // Allow normal input after the commit.
                    self.ivars().ime_state.set(ImeState::Ground);
                    true
                }
                ImeState::Preedit => true,
                // `key_down` could result in preedit clear, so compare old and current state.
                _ => old_ime_state != self.ivars().ime_state.get(),
            };

            if !had_ime_input || self.ivars().forward_key_to_app.get() {
                let key_event = create_key_event(&event, true, unsafe { event.isARepeat() }, None);
                self.queue_event(WindowEvent::KeyboardInput {
                    device_id: DEVICE_ID,
                    event: key_event,
                    is_synthetic: false,
                });
            }

            // Clear the flag after processing
            self.ivars().in_key_event.set(false);
        }

        #[method(keyUp:)]
        fn key_up(&self, event: &NSEvent) {
            trace_scope!("keyUp:");

            // Set flag to indicate we're in a key event
            self.ivars().in_key_event.set(true);

            let event = replace_event(event, self.option_as_alt());
            self.update_modifiers(&event, false);

            // We want to send keyboard input when we are currently in the ground state.
            if matches!(
                self.ivars().ime_state.get(),
                ImeState::Ground | ImeState::Disabled
            ) {
                self.queue_event(WindowEvent::KeyboardInput {
                    device_id: DEVICE_ID,
                    event: create_key_event(&event, false, false, None),
                    is_synthetic: false,
                });
            }

            // Clear the flag after processing
            self.ivars().in_key_event.set(false);
        }

        #[method(flagsChanged:)]
        fn flags_changed(&self, event: &NSEvent) {
            trace_scope!("flagsChanged:");

            self.update_modifiers(event, true);
        }

        #[method(insertTab:)]
        fn insert_tab(&self, _sender: Option<&AnyObject>) {
            trace_scope!("insertTab:");
            let window = self.window();
            if let Some(first_responder) = window.firstResponder() {
                if *first_responder == ***self {
                    window.selectNextKeyView(Some(self))
                }
            }
        }

        #[method(insertBackTab:)]
        fn insert_back_tab(&self, _sender: Option<&AnyObject>) {
            trace_scope!("insertBackTab:");
            let window = self.window();
            if let Some(first_responder) = window.firstResponder() {
                if *first_responder == ***self {
                    window.selectPreviousKeyView(Some(self))
                }
            }
        }

        // Allows us to receive Cmd-. (the shortcut for closing a dialog)
        // https://bugs.eclipse.org/bugs/show_bug.cgi?id=300620#c6
        #[method(cancelOperation:)]
        fn cancel_operation(&self, _sender: Option<&AnyObject>) {
            let mtm = MainThreadMarker::from(self);
            trace_scope!("cancelOperation:");

            let event = NSApplication::sharedApplication(mtm)
                .currentEvent()
                .expect("could not find current event");

            self.update_modifiers(&event, false);
            let event = create_key_event(&event, true, unsafe { event.isARepeat() }, None);

            self.queue_event(WindowEvent::KeyboardInput {
                device_id: DEVICE_ID,
                event,
                is_synthetic: false,
            });
        }

        // In the past (?), `mouseMoved:` events were not generated when the
        // user hovered over a window from a separate window, and as such the
        // application might not know the location of the mouse in the event.
        //
        // To fix this, we emit `mouse_motion` inside of mouse click, mouse
        // scroll, magnify and other gesture event handlers, to ensure that
        // the application's state of where the mouse click was located is up
        // to date.
        //
        // See https://github.com/rust-windowing/winit/pull/1490 for history.

        #[method(mouseDown:)]
        fn mouse_down(&self, event: &NSEvent) {
            trace_scope!("mouseDown:");
            self.mouse_motion(event);
            self.mouse_click(event, ElementState::Pressed);
        }

        #[method(mouseUp:)]
        fn mouse_up(&self, event: &NSEvent) {
            trace_scope!("mouseUp:");
            self.mouse_motion(event);
            self.mouse_click(event, ElementState::Released);
        }

        #[method(rightMouseDown:)]
        fn right_mouse_down(&self, event: &NSEvent) {
            trace_scope!("rightMouseDown:");
            self.mouse_motion(event);
            self.mouse_click(event, ElementState::Pressed);
        }

        #[method(rightMouseUp:)]
        fn right_mouse_up(&self, event: &NSEvent) {
            trace_scope!("rightMouseUp:");
            self.mouse_motion(event);
            self.mouse_click(event, ElementState::Released);
        }

        #[method(otherMouseDown:)]
        fn other_mouse_down(&self, event: &NSEvent) {
            trace_scope!("otherMouseDown:");
            self.mouse_motion(event);
            self.mouse_click(event, ElementState::Pressed);
        }

        #[method(otherMouseUp:)]
        fn other_mouse_up(&self, event: &NSEvent) {
            trace_scope!("otherMouseUp:");
            self.mouse_motion(event);
            self.mouse_click(event, ElementState::Released);
        }

        // No tracing on these because that would be overly verbose

        #[method(mouseMoved:)]
        fn mouse_moved(&self, event: &NSEvent) {
            self.mouse_motion(event);
        }

        #[method(mouseDragged:)]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.mouse_motion(event);
        }

        #[method(rightMouseDragged:)]
        fn right_mouse_dragged(&self, event: &NSEvent) {
            self.mouse_motion(event);
        }

        #[method(otherMouseDragged:)]
        fn other_mouse_dragged(&self, event: &NSEvent) {
            self.mouse_motion(event);
        }

        #[method(mouseEntered:)]
        fn mouse_entered(&self, _event: &NSEvent) {
            trace_scope!("mouseEntered:");
            self.queue_event(WindowEvent::CursorEntered {
                device_id: DEVICE_ID,
            });
        }

        #[method(mouseExited:)]
        fn mouse_exited(&self, _event: &NSEvent) {
            trace_scope!("mouseExited:");

            self.queue_event(WindowEvent::CursorLeft {
                device_id: DEVICE_ID,
            });
        }

        #[method(scrollWheel:)]
        fn scroll_wheel(&self, event: &NSEvent) {
            trace_scope!("scrollWheel:");

            self.mouse_motion(event);

            let delta = {
                let (x, y) = unsafe { (event.scrollingDeltaX(), event.scrollingDeltaY()) };
                if unsafe { event.hasPreciseScrollingDeltas() } {
                    let delta = LogicalPosition::new(x, y).to_physical(self.scale_factor());
                    MouseScrollDelta::PixelDelta(delta)
                } else {
                    MouseScrollDelta::LineDelta(x as f32, y as f32)
                }
            };

            // The "momentum phase," if any, has higher priority than touch phase (the two should
            // be mutually exclusive anyhow, which is why the API is rather incoherent). If no momentum
            // phase is recorded (or rather, the started/ended cases of the momentum phase) then we
            // report the touch phase.
            #[allow(non_upper_case_globals)]
            let phase = match unsafe { event.momentumPhase() } {
                NSEventPhase::MayBegin | NSEventPhase::Began => TouchPhase::Started,
                NSEventPhase::Ended | NSEventPhase::Cancelled => TouchPhase::Ended,
                _ => match unsafe { event.phase() } {
                    NSEventPhase::MayBegin | NSEventPhase::Began => TouchPhase::Started,
                    NSEventPhase::Ended | NSEventPhase::Cancelled => TouchPhase::Ended,
                    _ => TouchPhase::Moved,
                },
            };

            self.update_modifiers(event, false);

            self.queue_device_event(DeviceEvent::MouseWheel { delta });
            self.queue_event(WindowEvent::MouseWheel {
                device_id: DEVICE_ID,
                delta,
                phase,
            });
        }

        #[method(magnifyWithEvent:)]
        fn magnify_with_event(&self, event: &NSEvent) {
            trace_scope!("magnifyWithEvent:");

            self.mouse_motion(event);

            #[allow(non_upper_case_globals)]
            let phase = match unsafe { event.phase() } {
                NSEventPhase::Began => TouchPhase::Started,
                NSEventPhase::Changed => TouchPhase::Moved,
                NSEventPhase::Cancelled => TouchPhase::Cancelled,
                NSEventPhase::Ended => TouchPhase::Ended,
                _ => return,
            };

            self.queue_event(WindowEvent::PinchGesture {
                device_id: DEVICE_ID,
                delta: unsafe { event.magnification() },
                phase,
            });
        }

        #[method(smartMagnifyWithEvent:)]
        fn smart_magnify_with_event(&self, event: &NSEvent) {
            trace_scope!("smartMagnifyWithEvent:");

            self.mouse_motion(event);

            self.queue_event(WindowEvent::DoubleTapGesture {
                device_id: DEVICE_ID,
            });
        }

        #[method(rotateWithEvent:)]
        fn rotate_with_event(&self, event: &NSEvent) {
            trace_scope!("rotateWithEvent:");

            self.mouse_motion(event);

            #[allow(non_upper_case_globals)]
            let phase = match unsafe { event.phase() } {
                NSEventPhase::Began => TouchPhase::Started,
                NSEventPhase::Changed => TouchPhase::Moved,
                NSEventPhase::Cancelled => TouchPhase::Cancelled,
                NSEventPhase::Ended => TouchPhase::Ended,
                _ => return,
            };

            self.queue_event(WindowEvent::RotationGesture {
                device_id: DEVICE_ID,
                delta: unsafe { event.rotation() },
                phase,
            });
        }

        #[method(pressureChangeWithEvent:)]
        fn pressure_change_with_event(&self, event: &NSEvent) {
            trace_scope!("pressureChangeWithEvent:");

            self.queue_event(WindowEvent::TouchpadPressure {
                device_id: DEVICE_ID,
                pressure: unsafe { event.pressure() },
                stage: unsafe { event.stage() } as i64,
            });
        }

        // Allows us to receive Ctrl-Tab and Ctrl-Esc.
        // Note that this *doesn't* help with any missing Cmd inputs.
        // https://github.com/chromium/chromium/blob/a86a8a6bcfa438fa3ac2eba6f02b3ad1f8e0756f/ui/views/cocoa/bridged_content_view.mm#L816
        #[method(_wantsKeyDownForEvent:)]
        fn wants_key_down_for_event(&self, _event: &NSEvent) -> bool {
            trace_scope!("_wantsKeyDownForEvent:");
            true
        }

        #[method(acceptsFirstMouse:)]
        fn accepts_first_mouse(&self, _event: &NSEvent) -> bool {
            trace_scope!("acceptsFirstMouse:");
            self.ivars().accepts_first_mouse
        }
    }
);

impl WinitView {
    pub(super) fn new(
        app_delegate: &ApplicationDelegate,
        window: &WinitWindow,
        accepts_first_mouse: bool,
        option_as_alt: OptionAsAlt,
    ) -> Retained<Self> {
        let mtm = MainThreadMarker::from(window);
        let this = mtm.alloc().set_ivars(ViewState {
            app_delegate: app_delegate.retain(),
            cursor_state: Default::default(),
            ime_position: Default::default(),
            ime_size: Default::default(),
            modifiers: Default::default(),
            phys_modifiers: Default::default(),
            tracking_rect: Default::default(),
            ime_state: Default::default(),
            input_source: Default::default(),
            ime_allowed: Default::default(),
            forward_key_to_app: Default::default(),
            in_key_event: Default::default(),
            marked_text: Default::default(),
            accepts_first_mouse,
            _ns_window: WeakId::new(&window.retain()),
            option_as_alt: Cell::new(option_as_alt),
        });
        let this: Retained<Self> = unsafe { msg_send_id![super(this), init] };

        this.setPostsFrameChangedNotifications(true);
        let notification_center = unsafe { NSNotificationCenter::defaultCenter() };
        unsafe {
            notification_center.addObserver_selector_name_object(
                &this,
                sel!(frameDidChange:),
                Some(NSViewFrameDidChangeNotification),
                Some(&this),
            )
        }

        *this.ivars().input_source.borrow_mut() = this.current_input_source();

        this
    }

    fn window(&self) -> Retained<WinitWindow> {
        // TODO: Simply use `window` property on `NSView`.
        // That only returns a window _after_ the view has been attached though!
        // (which is incompatible with `frameDidChange:`)
        //
        // unsafe { msg_send_id![self, window] }
        self.ivars()
            ._ns_window
            .load()
            .expect("view to have a window")
    }

    fn queue_event(&self, event: WindowEvent) {
        self.ivars()
            .app_delegate
            .queue_window_event(self.window().id(), event);
    }

    fn queue_device_event(&self, event: DeviceEvent) {
        self.ivars().app_delegate.queue_device_event(event);
    }

    fn scale_factor(&self) -> f64 {
        self.window().backingScaleFactor() as f64
    }

    fn is_ime_enabled(&self) -> bool {
        !matches!(self.ivars().ime_state.get(), ImeState::Disabled)
    }

    fn current_input_source(&self) -> String {
        self.inputContext()
            .expect("input context")
            .selectedKeyboardInputSource()
            .map(|input_source| input_source.to_string())
            .unwrap_or_default()
    }

    pub(super) fn cursor_icon(&self) -> Retained<NSCursor> {
        self.ivars().cursor_state.borrow().cursor.clone()
    }

    pub(super) fn set_cursor_icon(&self, icon: Retained<NSCursor>) {
        let mut cursor_state = self.ivars().cursor_state.borrow_mut();
        cursor_state.cursor = icon;
    }

    /// Set whether the cursor should be visible or not.
    ///
    /// Returns whether the state changed.
    pub(super) fn set_cursor_visible(&self, visible: bool) -> bool {
        let mut cursor_state = self.ivars().cursor_state.borrow_mut();
        if visible != cursor_state.visible {
            cursor_state.visible = visible;
            true
        } else {
            false
        }
    }

    pub(super) fn set_ime_allowed(&self, ime_allowed: bool) {
        if self.ivars().ime_allowed.get() == ime_allowed {
            return;
        }
        self.ivars().ime_allowed.set(ime_allowed);
        if self.ivars().ime_allowed.get() {
            return;
        }

        // Clear markedText
        *self.ivars().marked_text.borrow_mut() = NSMutableAttributedString::new();

        if self.ivars().ime_state.get() != ImeState::Disabled {
            self.ivars().ime_state.set(ImeState::Disabled);
            self.queue_event(WindowEvent::Ime(Ime::Disabled));
        }
    }

    pub(super) fn set_ime_cursor_area(&self, position: NSPoint, size: NSSize) {
        // Validate coordinates to prevent invalid values that could cause IMK errors
        if position.x.is_nan()
            || position.y.is_nan()
            || position.x < 0.0
            || position.y < 0.0
        {
            tracing::warn!("Invalid IME cursor position: {:?}", position);
            return;
        }

        if size.width.is_nan()
            || size.height.is_nan()
            || size.width <= 0.0
            || size.height <= 0.0
        {
            tracing::warn!("Invalid IME cursor size: {:?}", size);
            return;
        }

        // Only update if position or size has actually changed to avoid unnecessary IMK calls
        let current_position = self.ivars().ime_position.get();
        let current_size = self.ivars().ime_size.get();

        if (current_position.x - position.x).abs() < 1.0
            && (current_position.y - position.y).abs() < 1.0
            && (current_size.width - size.width).abs() < 1.0
            && (current_size.height - size.height).abs() < 1.0
        {
            return; // No significant change, skip update
        }

        self.ivars().ime_position.set(position);
        self.ivars().ime_size.set(size);

        // Only invalidate if we have an input context and IME is enabled
        if let Some(input_context) = self.inputContext() {
            if self.is_ime_enabled() {
                input_context.invalidateCharacterCoordinates();
            }
        }
    }

    /// Reset modifiers and emit a synthetic ModifiersChanged event if deemed necessary.
    pub(super) fn reset_modifiers(&self) {
        if !self.ivars().modifiers.get().state().is_empty() {
            self.ivars().modifiers.set(Modifiers::default());
            self.queue_event(WindowEvent::ModifiersChanged(self.ivars().modifiers.get()));
        }
    }

    pub(super) fn set_option_as_alt(&self, value: OptionAsAlt) {
        self.ivars().option_as_alt.set(value)
    }

    pub(super) fn option_as_alt(&self) -> OptionAsAlt {
        self.ivars().option_as_alt.get()
    }

    /// Update modifiers if `event` has something different
    fn update_modifiers(&self, ns_event: &NSEvent, is_flags_changed_event: bool) {
        use ElementState::{Pressed, Released};

        let current_modifiers = event_mods(ns_event);
        let prev_modifiers = self.ivars().modifiers.get();
        self.ivars().modifiers.set(current_modifiers);

        // This function was called form the flagsChanged event, which is triggered
        // when the user presses/releases a modifier even if the same kind of modifier
        // has already been pressed.
        //
        // When flags changed event has key code of zero it means that event doesn't carry any key
        // event, thus we can't generate regular presses based on that. The `ModifiersChanged`
        // later will work though, since the flags are attached to the event and contain valid
        // information.
        'send_event: {
            if is_flags_changed_event && unsafe { ns_event.keyCode() } != 0 {
                let scancode = unsafe { ns_event.keyCode() };
                let physical_key = scancode_to_physicalkey(scancode as u32);

                // We'll correct the `is_press` later.
                let mut event =
                    create_key_event(ns_event, false, false, Some(physical_key));

                let key = code_to_key(physical_key, scancode);
                // Ignore processing of unknown modifiers because we can't determine whether
                // it was pressed or release reliably.
                let Some(event_modifier) = key_to_modifier(&key) else {
                    break 'send_event;
                };
                event.physical_key = physical_key;
                event.logical_key = key.clone();
                event.location = code_to_location(physical_key);
                let location_mask = ModLocationMask::from_location(event.location);

                let mut phys_mod_state = self.ivars().phys_modifiers.borrow_mut();
                let phys_mod = phys_mod_state
                    .entry(key)
                    .or_insert(ModLocationMask::empty());

                let is_active = current_modifiers.state().contains(event_modifier);
                let mut events = VecDeque::with_capacity(2);

                // There is no API for getting whether the button was pressed or released
                // during this event. For this reason we have to do a bit of magic below
                // to come up with a good guess whether this key was pressed or released.
                // (This is not trivial because there are multiple buttons that may affect
                // the same modifier)
                if !is_active {
                    event.state = Released;
                    if phys_mod.contains(ModLocationMask::LEFT) {
                        let mut event = event.clone();
                        event.location = KeyLocation::Left;
                        event.physical_key =
                            get_left_modifier_code(&event.logical_key).into();
                        events.push_back(WindowEvent::KeyboardInput {
                            device_id: DEVICE_ID,
                            event,
                            is_synthetic: false,
                        });
                    }
                    if phys_mod.contains(ModLocationMask::RIGHT) {
                        event.location = KeyLocation::Right;
                        event.physical_key =
                            get_right_modifier_code(&event.logical_key).into();
                        events.push_back(WindowEvent::KeyboardInput {
                            device_id: DEVICE_ID,
                            event,
                            is_synthetic: false,
                        });
                    }
                    *phys_mod = ModLocationMask::empty();
                } else {
                    if *phys_mod == location_mask {
                        // Here we hit a contradiction:
                        // The modifier state was "changed" to active,
                        // yet the only pressed modifier key was the one that we
                        // just got a change event for.
                        // This seemingly means that the only pressed modifier is now released,
                        // but at the same time the modifier became active.
                        //
                        // But this scenario is possible if we released modifiers
                        // while the application was not in focus. (Because we don't
                        // get informed of modifier key events while the application
                        // is not focused)

                        // In this case we prioritize the information
                        // about the current modifier state which means
                        // that the button was pressed.
                        event.state = Pressed;
                    } else {
                        phys_mod.toggle(location_mask);
                        let is_pressed = phys_mod.contains(location_mask);
                        event.state = if is_pressed { Pressed } else { Released };
                    }

                    events.push_back(WindowEvent::KeyboardInput {
                        device_id: DEVICE_ID,
                        event,
                        is_synthetic: false,
                    });
                }

                drop(phys_mod_state);

                for event in events {
                    self.queue_event(event);
                }
            }
        }

        if prev_modifiers == current_modifiers {
            return;
        }

        self.queue_event(WindowEvent::ModifiersChanged(self.ivars().modifiers.get()));
    }

    fn mouse_click(&self, event: &NSEvent, button_state: ElementState) {
        let button = mouse_button(event);

        self.update_modifiers(event, false);

        self.queue_event(WindowEvent::MouseInput {
            device_id: DEVICE_ID,
            state: button_state,
            button,
        });
    }

    fn mouse_motion(&self, event: &NSEvent) {
        let window_point = unsafe { event.locationInWindow() };
        let view_point = self.convertPoint_fromView(window_point, None);
        let frame = self.frame();

        if view_point.x.is_sign_negative()
            || view_point.y.is_sign_negative()
            || view_point.x > frame.size.width
            || view_point.y > frame.size.height
        {
            let mouse_buttons_down = unsafe { NSEvent::pressedMouseButtons() };
            if mouse_buttons_down == 0 {
                // Point is outside of the client area (view) and no buttons are pressed
                return;
            }
        }

        let view_point = LogicalPosition::new(view_point.x, view_point.y);

        self.update_modifiers(event, false);

        self.queue_event(WindowEvent::CursorMoved {
            device_id: DEVICE_ID,
            position: view_point.to_physical(self.scale_factor()),
        });
    }
}

/// Get the mouse button from the NSEvent.
fn mouse_button(event: &NSEvent) -> MouseButton {
    // The buttonNumber property only makes sense for the mouse events:
    // NSLeftMouse.../NSRightMouse.../NSOtherMouse...
    // For the other events, it's always set to 0.
    // MacOS only defines the left, right and middle buttons, 3..=31 are left as generic buttons,
    // but 3 and 4 are very commonly used as Back and Forward by hardware vendors and applications.
    match unsafe { event.buttonNumber() } {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        2 => MouseButton::Middle,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        n => MouseButton::Other(n as u16),
    }
}

// NOTE: to get option as alt working we need to rewrite events
// we're getting from the operating system, which makes it
// impossible to provide such events as extra in `KeyEvent`.
fn replace_event(event: &NSEvent, option_as_alt: OptionAsAlt) -> Retained<NSEvent> {
    let ev_mods = event_mods(event).state;
    let ignore_alt_characters = match option_as_alt {
        OptionAsAlt::OnlyLeft if lalt_pressed(event) => true,
        OptionAsAlt::OnlyRight if ralt_pressed(event) => true,
        OptionAsAlt::Both if ev_mods.alt_key() => true,
        _ => false,
    } && !ev_mods.control_key()
        && !ev_mods.super_key();

    if ignore_alt_characters {
        let ns_chars = unsafe {
            event
                .charactersIgnoringModifiers()
                .expect("expected characters to be non-null")
        };

        unsafe {
            NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
                event.r#type(),
                event.locationInWindow(),
                event.modifierFlags(),
                event.timestamp(),
                event.windowNumber(),
                None,
                &ns_chars,
                &ns_chars,
                event.isARepeat(),
                event.keyCode(),
            )
            .unwrap()
        }
    } else {
        event.copy()
    }
}

/// Get the window delegate from a view pointer
/// This is used by the CVDisplayLink callback to access window state
pub(crate) unsafe fn get_window_delegate(
    view_ptr: *mut std::ffi::c_void,
) -> Option<Retained<WindowDelegate>> {
    if view_ptr.is_null() {
        return None;
    }

    unsafe {
        // Cast the view pointer back to WinitView
        let view = view_ptr as *const WinitView;
        let view = &*view;

        // Get the window from the view
        if let Some(window) = view.ivars()._ns_window.load() {
            // Get the delegate from the window
            if let Some(delegate) = window.delegate() {
                // Cast the delegate back to WindowDelegate
                // SAFETY: We know this is a WindowDelegate because we set it
                let delegate_ptr = Retained::as_ptr(&delegate) as *mut WindowDelegate;
                Retained::retain(delegate_ptr)
            } else {
                None
            }
        } else {
            None
        }
    }
}
