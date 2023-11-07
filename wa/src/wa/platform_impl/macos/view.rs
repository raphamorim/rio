// WA is a fork of https://github.com/rust-windowing/wa/
// wa is is licensed under Apache 2.0 license https://github.com/rust-windowing/wa/blob/master/LICENSE

#![allow(clippy::unnecessary_cast)]
use bitflags::bitflags;
use log::warn;
use std::boxed::Box;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::ptr::NonNull;

use icrate::Foundation::{
    NSArray, NSAttributedString, NSAttributedStringKey, NSCopying,
    NSMutableAttributedString, NSObject, NSObjectProtocol, NSPoint, NSRange, NSRect,
    NSSize, NSString, NSUInteger,
};
use objc2::declare::{Ivar, IvarDrop};
use objc2::rc::{Id, WeakId};
use objc2::runtime::{AnyObject, Sel};
use objc2::{class, declare_class, msg_send, msg_send_id, mutability, sel, ClassType};

use super::{
    appkit::{
        NSApp, NSCursor, NSEvent, NSEventPhase, NSResponder, NSTextInputClient,
        NSTrackingRectTag, NSView,
    },
    event::{code_to_key, code_to_location},
};
use crate::{
    dpi::{LogicalPosition, LogicalSize},
    event::{
        DeviceEvent, ElementState, Event, Ime, Modifiers, MouseButton, MouseScrollDelta,
        TouchPhase, WindowEvent,
    },
    keyboard::{Key, KeyCode, KeyLocation, ModifiersState, NamedKey, PhysicalKey},
    platform::macos::{OptionAsAlt, WindowExtMacOS},
    platform::scancode::PhysicalKeyExtScancode,
    wa::platform_impl::platform::{
        app_state::AppState,
        event::{create_key_event, event_mods},
        util,
        window::waWindow,
        DEVICE_ID,
    },
    window::WindowId,
};

#[derive(Debug)]
struct CursorState {
    visible: bool,
    cursor: Id<NSCursor>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            visible: true,
            cursor: Default::default(),
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

    /// The text was just commited, so the next input from the keyboard must be ignored.
    Commited,
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct ModLocationMask: u8 {
        const LEFT = 1;
        const RIGHT = 2;
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

fn key_to_modifier(key: &Key) -> ModifiersState {
    match key {
        Key::Named(NamedKey::Alt) => ModifiersState::ALT,
        Key::Named(NamedKey::Control) => ModifiersState::CONTROL,
        Key::Named(NamedKey::Super) => ModifiersState::SUPER,
        Key::Named(NamedKey::Shift) => ModifiersState::SHIFT,
        _ => unreachable!(),
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

#[derive(Debug, Default)]
pub struct ViewState {
    cursor_state: RefCell<CursorState>,
    ime_position: Cell<LogicalPosition<f64>>,
    ime_size: Cell<LogicalSize<f64>>,
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

    marked_text: RefCell<Id<NSMutableAttributedString>>,
    accepts_first_mouse: bool,
}

declare_class!(
    #[derive(Debug)]
    #[allow(non_snake_case)]
    pub(super) struct waView {
        // Weak reference because the window keeps a strong reference to the view
        _ns_window: IvarDrop<Box<WeakId<waWindow>>, "__ns_window">,
        state: IvarDrop<Box<ViewState>, "_state">,
    }

    mod ivars;

    unsafe impl ClassType for waView {
        #[inherits(NSResponder, NSObject)]
        type Super = NSView;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "waView";
    }

    unsafe impl waView {
        #[method(initWithId:acceptsFirstMouse:)]
        unsafe fn init_with_id(
            this: *mut Self,
            window: &waWindow,
            accepts_first_mouse: bool,
        ) -> Option<NonNull<Self>> {
            let this: Option<&mut Self> = unsafe { msg_send![super(this), init] };
            this.map(|this| {
                let state = ViewState {
                    accepts_first_mouse,
                    ..Default::default()
                };

                Ivar::write(
                    &mut this._ns_window,
                    Box::new(WeakId::new(&window.retain())),
                );
                Ivar::write(&mut this.state, Box::new(state));

                this.setPostsFrameChangedNotifications(true);

                let notification_center: &AnyObject =
                    unsafe { msg_send![class!(NSNotificationCenter), defaultCenter] };
                // About frame change
                let frame_did_change_notification_name =
                    NSString::from_str("NSViewFrameDidChangeNotification");
                #[allow(clippy::let_unit_value)]
                unsafe {
                    let _: () = msg_send![
                        notification_center,
                        addObserver: &*this,
                        selector: sel!(frameDidChange:),
                        name: &*frame_did_change_notification_name,
                        object: &*this,
                    ];
                }

                *this.state.input_source.borrow_mut() = this.current_input_source();
                NonNull::from(this)
            })
        }
    }

    unsafe impl waView {
        #[method(viewDidMoveToWindow)]
        fn view_did_move_to_window(&self) {
            trace_scope!("viewDidMoveToWindow");
            if let Some(tracking_rect) = self.state.tracking_rect.take() {
                self.removeTrackingRect(tracking_rect);
            }

            let rect = self.frame();
            let tracking_rect = self.add_tracking_rect(rect, false);
            self.state.tracking_rect.set(Some(tracking_rect));
        }

        #[method(frameDidChange:)]
        fn frame_did_change(&self, _event: &NSEvent) {
            trace_scope!("frameDidChange:");
            if let Some(tracking_rect) = self.state.tracking_rect.take() {
                self.removeTrackingRect(tracking_rect);
            }

            let rect = self.frame();
            let tracking_rect = self.add_tracking_rect(rect, false);
            self.state.tracking_rect.set(Some(tracking_rect));

            // Emit resize event here rather than from windowDidResize because:
            // 1. When a new window is created as a tab, the frame size may change without a window resize occurring.
            // 2. Even when a window resize does occur on a new tabbed window, it contains the wrong size (includes tab height).
            let logical_size =
                LogicalSize::new(rect.size.width as f64, rect.size.height as f64);
            let size = logical_size.to_physical::<u32>(self.scale_factor());
            self.queue_event(WindowEvent::Resized(size));
        }

        #[method(drawRect:)]
        fn draw_rect(&self, rect: NSRect) {
            trace_scope!("drawRect:");

            // It's a workaround for https://github.com/rust-windowing/wa/issues/2640, don't replace with `self.window_id()`.
            if let Some(window) = self._ns_window.load() {
                AppState::handle_redraw(WindowId(window.id()));
            }

            #[allow(clippy::let_unit_value)]
            unsafe {
                let _: () = msg_send![super(self), drawRect: rect];
            }
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
        fn touch_bar(&self) -> Option<Id<NSObject>> {
            trace_scope!("touchBar");
            None
        }

        #[method(resetCursorRects)]
        fn reset_cursor_rects(&self) {
            trace_scope!("resetCursorRects");
            let bounds = self.bounds();
            let cursor_state = self.state.cursor_state.borrow();
            // We correctly invoke `addCursorRect` only from inside `resetCursorRects`
            if cursor_state.visible {
                self.addCursorRect(bounds, &cursor_state.cursor);
            } else {
                self.addCursorRect(bounds, &NSCursor::invisible());
            }
        }
    }

    unsafe impl NSTextInputClient for waView {
        #[method(hasMarkedText)]
        fn has_marked_text(&self) -> bool {
            trace_scope!("hasMarkedText");
            self.state.marked_text.borrow().length() > 0
        }

        #[method(markedRange)]
        fn marked_range(&self) -> NSRange {
            trace_scope!("markedRange");
            let length = self.state.marked_text.borrow().length();
            if length > 0 {
                NSRange::new(0, length)
            } else {
                util::EMPTY_RANGE
            }
        }

        #[method(selectedRange)]
        fn selected_range(&self) -> NSRange {
            trace_scope!("selectedRange");
            util::EMPTY_RANGE
        }

        #[method(setMarkedText:selectedRange:replacementRange:)]
        fn set_marked_text(
            &self,
            string: &NSObject,
            _selected_range: NSRange,
            _replacement_range: NSRange,
        ) {
            trace_scope!("setMarkedText:selectedRange:replacementRange:");

            // SAFETY: This method is guaranteed to get either a `NSString` or a `NSAttributedString`.
            let (marked_text, preedit_string) =
                if string.is_kind_of::<NSAttributedString>() {
                    let string: *const NSObject = string;
                    let string: *const NSAttributedString = string.cast();
                    let string = unsafe { &*string };
                    (
                        NSMutableAttributedString::from_attributed_nsstring(string),
                        string.string().to_string(),
                    )
                } else {
                    let string: *const NSObject = string;
                    let string: *const NSString = string.cast();
                    let string = unsafe { &*string };
                    (
                        NSMutableAttributedString::from_nsstring(string),
                        string.to_string(),
                    )
                };

            // Update marked text.
            *self.state.marked_text.borrow_mut() = marked_text;

            // Notify IME is active if application still doesn't know it.
            if self.state.ime_state.get() == ImeState::Disabled {
                *self.state.input_source.borrow_mut() = self.current_input_source();
                self.queue_event(WindowEvent::Ime(Ime::Enabled));
            }

            if self.hasMarkedText() {
                self.state.ime_state.set(ImeState::Preedit);
            } else {
                // In case the preedit was cleared, set IME into the Ground state.
                self.state.ime_state.set(ImeState::Ground);
            }

            // Empty string basically means that there's no preedit, so indicate that by sending
            // `None` cursor range.
            let cursor_range = if preedit_string.is_empty() {
                None
            } else {
                Some((preedit_string.len(), preedit_string.len()))
            };

            // Send WindowEvent for updating marked text
            self.queue_event(WindowEvent::Ime(Ime::Preedit(
                preedit_string,
                cursor_range,
            )));
        }

        #[method(unmarkText)]
        fn unmark_text(&self) {
            trace_scope!("unmarkText");
            *self.state.marked_text.borrow_mut() = NSMutableAttributedString::new();

            let input_context = self.inputContext().expect("input context");
            input_context.discardMarkedText();

            self.queue_event(WindowEvent::Ime(Ime::Preedit(String::new(), None)));
            if self.is_ime_enabled() {
                // Leave the Preedit self.state
                self.state.ime_state.set(ImeState::Ground);
            } else {
                warn!("Expected to have IME enabled when receiving unmarkText");
            }
        }

        #[method_id(validAttributesForMarkedText)]
        fn valid_attributes_for_marked_text(&self) -> Id<NSArray<NSAttributedStringKey>> {
            trace_scope!("validAttributesForMarkedText");
            NSArray::new()
        }

        #[method_id(attributedSubstringForProposedRange:actualRange:)]
        fn attributed_substring_for_proposed_range(
            &self,
            _range: NSRange,
            _actual_range: *mut NSRange,
        ) -> Option<Id<NSAttributedString>> {
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
            let window = self.window();
            let content_rect = window.contentRectForFrameRect(window.frame());
            let base_x = content_rect.origin.x as f64;
            let base_y = (content_rect.origin.y + content_rect.size.height) as f64;
            let x = base_x + self.state.ime_position.get().x;
            let y = base_y - self.state.ime_position.get().y;
            let LogicalSize { width, height } = self.state.ime_size.get();
            NSRect::new(NSPoint::new(x as _, y as _), NSSize::new(width, height))
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

            let is_control = string.chars().next().map_or(false, |c| c.is_control());

            // Commit only if we have marked text.
            if self.hasMarkedText() && self.is_ime_enabled() && !is_control {
                self.queue_event(WindowEvent::Ime(Ime::Preedit(String::new(), None)));
                self.queue_event(WindowEvent::Ime(Ime::Commit(string)));
                self.state.ime_state.set(ImeState::Commited);
            }
        }

        // Basically, we're sent this message whenever a keyboard event that doesn't generate a "human
        // readable" character happens, i.e. newlines, tabs, and Ctrl+C.
        #[method(doCommandBySelector:)]
        fn do_command_by_selector(&self, _command: Sel) {
            trace_scope!("doCommandBySelector:");
            // We shouldn't forward any character from just commited text, since we'll end up sending
            // it twice with some IMEs like Korean one. We'll also always send `Enter` in that case,
            // which is not desired given it was used to confirm IME input.
            if self.state.ime_state.get() == ImeState::Commited {
                return;
            }

            self.state.forward_key_to_app.set(true);

            if self.hasMarkedText() && self.state.ime_state.get() == ImeState::Preedit {
                // Leave preedit so that we also report the key-up for this key.
                self.state.ime_state.set(ImeState::Ground);
            }
        }
    }

    unsafe impl waView {
        #[method(keyDown:)]
        fn key_down(&self, event: &NSEvent) {
            trace_scope!("keyDown:");
            {
                let mut prev_input_source = self.state.input_source.borrow_mut();
                let current_input_source = self.current_input_source();
                if *prev_input_source != current_input_source && self.is_ime_enabled() {
                    *prev_input_source = current_input_source;
                    drop(prev_input_source);
                    self.state.ime_state.set(ImeState::Disabled);
                    self.queue_event(WindowEvent::Ime(Ime::Disabled));
                }
            }

            // Get the characters from the event.
            let old_ime_state = self.state.ime_state.get();
            self.state.forward_key_to_app.set(false);
            let event = replace_event(event, self.window().option_as_alt());

            // The `interpretKeyEvents` function might call
            // `setMarkedText`, `insertText`, and `doCommandBySelector`.
            // It's important that we call this before queuing the KeyboardInput, because
            // we must send the `KeyboardInput` event during IME if it triggered
            // `doCommandBySelector`. (doCommandBySelector means that the keyboard input
            // is not handled by IME and should be handled by the application)
            if self.state.ime_allowed.get() {
                let events_for_nsview = NSArray::from_slice(&[&*event]);
                unsafe { self.interpretKeyEvents(&events_for_nsview) };

                // If the text was commited we must treat the next keyboard event as IME related.
                if self.state.ime_state.get() == ImeState::Commited {
                    // Remove any marked text, so normal input can continue.
                    *self.state.marked_text.borrow_mut() =
                        NSMutableAttributedString::new();
                }
            }

            self.update_modifiers(&event, false);

            let had_ime_input = match self.state.ime_state.get() {
                ImeState::Commited => {
                    // Allow normal input after the commit.
                    self.state.ime_state.set(ImeState::Ground);
                    true
                }
                ImeState::Preedit => true,
                // `key_down` could result in preedit clear, so compare old and current state.
                _ => old_ime_state != self.state.ime_state.get(),
            };

            if !had_ime_input || self.state.forward_key_to_app.get() {
                let key_event = create_key_event(&event, true, event.is_a_repeat(), None);
                self.queue_event(WindowEvent::KeyboardInput {
                    device_id: DEVICE_ID,
                    event: key_event,
                    is_synthetic: false,
                });
            }
        }

        #[method(keyUp:)]
        fn key_up(&self, event: &NSEvent) {
            trace_scope!("keyUp:");

            let event = replace_event(event, self.window().option_as_alt());
            self.update_modifiers(&event, false);

            // We want to send keyboard input when we are currently in the ground state.
            if matches!(
                self.state.ime_state.get(),
                ImeState::Ground | ImeState::Disabled
            ) {
                self.queue_event(WindowEvent::KeyboardInput {
                    device_id: DEVICE_ID,
                    event: create_key_event(&event, false, false, None),
                    is_synthetic: false,
                });
            }
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
            trace_scope!("cancelOperation:");

            let event = NSApp()
                .currentEvent()
                .expect("could not find current event");

            self.update_modifiers(&event, false);
            let event = create_key_event(&event, true, event.is_a_repeat(), None);

            self.queue_event(WindowEvent::KeyboardInput {
                device_id: DEVICE_ID,
                event,
                is_synthetic: false,
            });
        }

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
                let (x, y) = (event.scrollingDeltaX(), event.scrollingDeltaY());
                if event.hasPreciseScrollingDeltas() {
                    let delta =
                        LogicalPosition::new(x, y).to_physical(self.scale_factor());
                    MouseScrollDelta::PixelDelta(delta)
                } else {
                    MouseScrollDelta::LineDelta(x as f32, y as f32)
                }
            };

            // The "momentum phase," if any, has higher priority than touch phase (the two should
            // be mutually exclusive anyhow, which is why the API is rather incoherent). If no momentum
            // phase is recorded (or rather, the started/ended cases of the momentum phase) then we
            // report the touch phase.
            let phase =
                match event.momentumPhase() {
                    NSEventPhase::NSEventPhaseMayBegin
                    | NSEventPhase::NSEventPhaseBegan => TouchPhase::Started,
                    NSEventPhase::NSEventPhaseEnded
                    | NSEventPhase::NSEventPhaseCancelled => TouchPhase::Ended,
                    _ => match event.phase() {
                        NSEventPhase::NSEventPhaseMayBegin
                        | NSEventPhase::NSEventPhaseBegan => TouchPhase::Started,
                        NSEventPhase::NSEventPhaseEnded
                        | NSEventPhase::NSEventPhaseCancelled => TouchPhase::Ended,
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

            let phase = match event.phase() {
                NSEventPhase::NSEventPhaseBegan => TouchPhase::Started,
                NSEventPhase::NSEventPhaseChanged => TouchPhase::Moved,
                NSEventPhase::NSEventPhaseCancelled => TouchPhase::Cancelled,
                NSEventPhase::NSEventPhaseEnded => TouchPhase::Ended,
                _ => return,
            };

            self.queue_event(WindowEvent::TouchpadMagnify {
                device_id: DEVICE_ID,
                delta: event.magnification(),
                phase,
            });
        }

        #[method(smartMagnifyWithEvent:)]
        fn smart_magnify_with_event(&self, _event: &NSEvent) {
            trace_scope!("smartMagnifyWithEvent:");

            self.queue_event(WindowEvent::SmartMagnify {
                device_id: DEVICE_ID,
            });
        }

        #[method(rotateWithEvent:)]
        fn rotate_with_event(&self, event: &NSEvent) {
            trace_scope!("rotateWithEvent:");

            let phase = match event.phase() {
                NSEventPhase::NSEventPhaseBegan => TouchPhase::Started,
                NSEventPhase::NSEventPhaseChanged => TouchPhase::Moved,
                NSEventPhase::NSEventPhaseCancelled => TouchPhase::Cancelled,
                NSEventPhase::NSEventPhaseEnded => TouchPhase::Ended,
                _ => return,
            };

            self.queue_event(WindowEvent::TouchpadRotate {
                device_id: DEVICE_ID,
                delta: event.rotation(),
                phase,
            });
        }

        #[method(pressureChangeWithEvent:)]
        fn pressure_change_with_event(&self, event: &NSEvent) {
            trace_scope!("pressureChangeWithEvent:");

            self.mouse_motion(event);

            self.queue_event(WindowEvent::TouchpadPressure {
                device_id: DEVICE_ID,
                pressure: event.pressure(),
                stage: event.stage() as i64,
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
            self.state.accepts_first_mouse
        }
    }
);

impl waView {
    pub(super) fn new(window: &waWindow, accepts_first_mouse: bool) -> Id<Self> {
        unsafe {
            msg_send_id![
                Self::alloc(),
                initWithId: window,
                acceptsFirstMouse: accepts_first_mouse,
            ]
        }
    }

    fn window(&self) -> Id<waWindow> {
        // TODO: Simply use `window` property on `NSView`.
        // That only returns a window _after_ the view has been attached though!
        // (which is incompatible with `frameDidChange:`)
        //
        // unsafe { msg_send_id![self, window] }
        self._ns_window.load().expect("view to have a window")
    }

    fn window_id(&self) -> WindowId {
        WindowId(self.window().id())
    }

    fn queue_event(&self, event: WindowEvent) {
        let event = Event::WindowEvent {
            window_id: self.window_id(),
            event,
        };
        AppState::queue_event(event);
    }

    fn queue_device_event(&self, event: DeviceEvent) {
        let event = Event::DeviceEvent {
            device_id: DEVICE_ID,
            event,
        };
        AppState::queue_event(event);
    }

    fn scale_factor(&self) -> f64 {
        self.window().backingScaleFactor() as f64
    }

    fn is_ime_enabled(&self) -> bool {
        !matches!(self.state.ime_state.get(), ImeState::Disabled)
    }

    fn current_input_source(&self) -> String {
        self.inputContext()
            .expect("input context")
            .selectedKeyboardInputSource()
            .map(|input_source| input_source.to_string())
            .unwrap_or_default()
    }

    pub(super) fn set_cursor_icon(&self, icon: Id<NSCursor>) {
        let mut cursor_state = self.state.cursor_state.borrow_mut();
        cursor_state.cursor = icon;
    }

    /// Set whether the cursor should be visible or not.
    ///
    /// Returns whether the state changed.
    pub(super) fn set_cursor_visible(&self, visible: bool) -> bool {
        let mut cursor_state = self.state.cursor_state.borrow_mut();
        if visible != cursor_state.visible {
            cursor_state.visible = visible;
            true
        } else {
            false
        }
    }

    pub(super) fn set_ime_allowed(&self, ime_allowed: bool) {
        if self.state.ime_allowed.get() == ime_allowed {
            return;
        }
        self.state.ime_allowed.set(ime_allowed);
        if self.state.ime_allowed.get() {
            return;
        }

        // Clear markedText
        *self.state.marked_text.borrow_mut() = NSMutableAttributedString::new();

        if self.state.ime_state.get() != ImeState::Disabled {
            self.state.ime_state.set(ImeState::Disabled);
            self.queue_event(WindowEvent::Ime(Ime::Disabled));
        }
    }

    pub(super) fn set_ime_cursor_area(
        &self,
        position: LogicalPosition<f64>,
        size: LogicalSize<f64>,
    ) {
        self.state.ime_position.set(position);
        self.state.ime_size.set(size);
        let input_context = self.inputContext().expect("input context");
        input_context.invalidateCharacterCoordinates();
    }

    /// Reset modifiers and emit a synthetic ModifiersChanged event if deemed necessary.
    pub(super) fn reset_modifiers(&self) {
        if !self.state.modifiers.get().state().is_empty() {
            self.state.modifiers.set(Modifiers::default());
            self.queue_event(WindowEvent::ModifiersChanged(self.state.modifiers.get()));
        }
    }

    /// Update modifiers if `event` has something different
    fn update_modifiers(&self, ns_event: &NSEvent, is_flags_changed_event: bool) {
        use ElementState::{Pressed, Released};

        let current_modifiers = event_mods(ns_event);
        let prev_modifiers = self.state.modifiers.get();
        self.state.modifiers.set(current_modifiers);

        // This function was called form the flagsChanged event, which is triggered
        // when the user presses/releases a modifier even if the same kind of modifier
        // has already been pressed.
        //
        // When flags changed event has key code of zero it means that event doesn't carry any key
        // event, thus we can't generate regular presses based on that. The `ModifiersChanged`
        // later will work though, since the flags are attached to the event and contain valid
        // information.
        if is_flags_changed_event && ns_event.key_code() != 0 {
            let scancode = ns_event.key_code();
            let physical_key = PhysicalKey::from_scancode(scancode as u32);

            // We'll correct the `is_press` later.
            let mut event = create_key_event(ns_event, false, false, Some(physical_key));

            let key = code_to_key(physical_key, scancode);
            let event_modifier = key_to_modifier(&key);
            event.physical_key = physical_key;
            event.logical_key = key.clone();
            event.location = code_to_location(physical_key);
            let location_mask = ModLocationMask::from_location(event.location);

            let mut phys_mod_state = self.state.phys_modifiers.borrow_mut();
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
                // is_active
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

        if prev_modifiers == current_modifiers {
            return;
        }

        self.queue_event(WindowEvent::ModifiersChanged(self.state.modifiers.get()));
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
        let window_point = event.locationInWindow();
        let view_point = self.convertPoint_fromView(window_point, None);
        let view_rect = self.frame();

        if view_point.x.is_sign_negative()
            || view_point.y.is_sign_negative()
            || view_point.x > view_rect.size.width
            || view_point.y > view_rect.size.height
        {
            let mouse_buttons_down = NSEvent::pressedMouseButtons();
            if mouse_buttons_down == 0 {
                // Point is outside of the client area (view) and no buttons are pressed
                return;
            }
        }

        let x = view_point.x as f64;
        let y = view_rect.size.height as f64 - view_point.y as f64;
        let logical_position = LogicalPosition::new(x, y);

        self.update_modifiers(event, false);

        self.queue_event(WindowEvent::CursorMoved {
            device_id: DEVICE_ID,
            position: logical_position.to_physical(self.scale_factor()),
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
    match event.buttonNumber() {
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
fn replace_event(event: &NSEvent, option_as_alt: OptionAsAlt) -> Id<NSEvent> {
    let ev_mods = event_mods(event).state;
    let ignore_alt_characters = match option_as_alt {
        OptionAsAlt::OnlyLeft if event.lalt_pressed() => true,
        OptionAsAlt::OnlyRight if event.ralt_pressed() => true,
        OptionAsAlt::Both if ev_mods.alt_key() => true,
        _ => false,
    } && !ev_mods.control_key()
        && !ev_mods.super_key();

    if ignore_alt_characters {
        let ns_chars = event
            .charactersIgnoringModifiers()
            .expect("expected characters to be non-null");

        NSEvent::keyEventWithType(
            event.type_(),
            event.locationInWindow(),
            event.modifierFlags(),
            event.timestamp(),
            event.window_number(),
            None,
            &ns_chars,
            &ns_chars,
            event.is_a_repeat(),
            event.key_code(),
        )
    } else {
        event.copy()
    }
}
