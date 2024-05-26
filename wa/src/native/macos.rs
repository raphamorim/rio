// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT
// https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT
// The code has suffered several changes like support to multiple windows, extension of windows
// properties, menu support, IME support, and etc.

#![allow(clippy::match_ref_pats)]

use crate::app::{EventLoopWaker, HandlerState};
use crate::event::{QueuedEvent, WindowEvent};
use crate::native::apple::menu::{KeyAssignment, Menu, MenuItem, RepresentedItem};
use crate::native::macos::NSEventMask::NSAnyEventMask;
use crate::native::macos::NSEventType::NSApplicationDefined;
use crate::{Appearance, EventHandlerControl};
use objc::rc::StrongPtr;
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::OnceLock;

use {
    crate::{
        event::{EventHandler, MouseButton},
        get_handler,
        native::{
            apple::{apple_util::*, frameworks::*},
            NativeDisplayData,
        },
        CursorIcon,
    },
    std::{collections::HashMap, os::raw::c_void},
};

#[allow(non_upper_case_globals)]
#[allow(unused)]
const NSViewLayerContentsPlacementTopLeft: isize = 11;
#[allow(non_upper_case_globals)]
#[allow(unused)]
const NSViewLayerContentsRedrawDuringViewResize: isize = 2;

const APP_STATE_IVAR_NAME: &str = "AppState";
const VIEW_IVAR_NAME: &str = "RioDisplay";
const VIEW_CLASS_NAME: &str = "RioViewWithId";

const NSNOT_FOUND: i32 = i32::MAX;

#[repr(i16)]
pub enum NSEventSubtype {
    // TODO: Not sure what these values are
    // NSMouseEventSubtype           = NX_SUBTYPE_DEFAULT,
    // NSTabletPointEventSubtype     = NX_SUBTYPE_TABLET_POINT,
    // NSTabletProximityEventSubtype = NX_SUBTYPE_TABLET_PROXIMITY
    // NSTouchEventSubtype           = NX_SUBTYPE_MOUSE_TOUCH,
    NSWindowExposedEventType = 0,
    NSApplicationActivatedEventType = 1,
    NSApplicationDeactivatedEventType = 2,
    NSWindowMovedEventType = 4,
    NSScreenChangedEventType = 8,
    NSAWTEventType = 16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u64)] // NSUInteger
pub enum NSEventType {
    NSLeftMouseDown = 1,
    NSLeftMouseUp = 2,
    NSRightMouseDown = 3,
    NSRightMouseUp = 4,
    NSMouseMoved = 5,
    NSLeftMouseDragged = 6,
    NSRightMouseDragged = 7,
    NSMouseEntered = 8,
    NSMouseExited = 9,
    NSKeyDown = 10,
    NSKeyUp = 11,
    NSFlagsChanged = 12,
    NSAppKitDefined = 13,
    NSSystemDefined = 14,
    NSApplicationDefined = 15,
    NSPeriodic = 16,
    NSCursorUpdate = 17,
    NSScrollWheel = 22,
    NSTabletPoint = 23,
    NSTabletProximity = 24,
    NSOtherMouseDown = 25,
    NSOtherMouseUp = 26,
    NSOtherMouseDragged = 27,
    NSEventTypeGesture = 29,
    NSEventTypeMagnify = 30,
    NSEventTypeSwipe = 31,
    NSEventTypeRotate = 18,
    NSEventTypeBeginGesture = 19,
    NSEventTypeEndGesture = 20,
    NSEventTypePressure = 34,
}

pub static NATIVE_APP: OnceLock<App> = OnceLock::new();

#[cfg(target_pointer_width = "32")]
pub type NSInteger = libc::c_int;
#[cfg(target_pointer_width = "32")]
pub type NSUInteger = libc::c_uint;

#[cfg(target_pointer_width = "64")]
pub type NSInteger = libc::c_long;
#[cfg(target_pointer_width = "64")]
pub type NSUInteger = libc::c_ulong;

#[derive(Debug, PartialEq)]
enum ImeState {
    // The IME events are disabled, so only `ReceivedCharacter` is being sent to the user.
    Disabled,

    // The ground state of enabled IME input. It means that both Preedit and regular keyboard
    // input could be start from it.
    Ground,

    // The IME is in preedit.
    Preedit,

    // The text was just commited, so the next input from the keyboard must be ignored.
    Commited,
}

#[derive(Default)]
pub struct AppState {
    pending_events: RefCell<VecDeque<QueuedEvent>>,
    waker: RefCell<EventLoopWaker>,
    handler: Option<HandlerState>,
}

impl AppState {
    pub fn new(handler: HandlerState) -> Self {
        Self {
            handler: Some(handler),
            ..Default::default()
        }
    }
}

pub struct MacosDisplay {
    window: ObjcId,
    view: ObjcId,
    app: ObjcId,
    renderer: *mut ObjcId,
    fullscreen: bool,
    id: u16,
    ime: ImeState,
    marked_text: String,
    // [NSCursor hide]/unhide calls should be balanced
    // hide/hide/unhide will keep cursor hidden
    // so need to keep internal cursor state to avoid problems from
    // unbalanced show_mouse() calls
    cursor_shown: bool,
    current_cursor: CursorIcon,
    cursor_grabbed: bool,
    cursors: HashMap<CursorIcon, ObjcId>,
    has_initialized: bool,
    has_focus: bool,
    modifiers: Modifiers,
    screen_width: i32,
    screen_height: i32,
    dpi_scale: f32,
    #[allow(unused)]
    high_dpi: bool,
}

impl MacosDisplay {
    fn raw_window_handle(&self) -> RawWindowHandle {
        let window_handle =
            AppKitWindowHandle::new(NonNull::new(self.view as *mut _).unwrap());
        // window_handle.ns_window = self.window as *mut _;
        // window_handle.ns_view = self.view as *mut _;
        RawWindowHandle::AppKit(window_handle)
    }

    fn raw_display_handle(&self) -> RawDisplayHandle {
        let handle = AppKitDisplayHandle::new();
        RawDisplayHandle::AppKit(handle)
    }
}

impl MacosDisplay {
    pub fn set_cursor_grab(&mut self, grab: bool) {
        if grab == self.cursor_grabbed {
            return;
        }

        self.cursor_grabbed = grab;

        unsafe {
            if grab {
                self.move_mouse_inside_window(self.window);
                CGAssociateMouseAndMouseCursorPosition(false);
                let () = msg_send![class!(NSCursor), hide];
            } else {
                let () = msg_send![class!(NSCursor), unhide];
                CGAssociateMouseAndMouseCursorPosition(true);
            }
        }
    }
    pub fn show_mouse(&mut self, show: bool) {
        if show && !self.cursor_shown {
            unsafe {
                let () = msg_send![class!(NSCursor), unhide];
            }
        }
        if !show && self.cursor_shown {
            unsafe {
                let () = msg_send![class!(NSCursor), hide];
            }
        }
        self.cursor_shown = show;
    }
    pub fn set_mouse_cursor(&mut self, cursor: crate::CursorIcon) {
        if self.current_cursor != cursor {
            self.current_cursor = cursor;
            unsafe {
                let _: () = msg_send![
                    self.window,
                    invalidateCursorRectsForView: self.view
                ];
            }
        }
    }
    pub fn set_title(&self, title: &str) {
        unsafe {
            let title = str_to_nsstring(title);
            let _: () = msg_send![&*self.window, setTitle: &*title];
        }
    }
    pub fn set_subtitle(&self, subtitle: &str) {
        // if !os::is_minimum_version(11) {
        //     return;
        // }

        unsafe {
            let subtitle = str_to_nsstring(subtitle);
            let _: () = msg_send![&*self.window, setSubtitle: &*subtitle];
        }
    }
    pub fn set_window_size(&mut self, new_width: u32, new_height: u32) {
        let mut frame: NSRect = unsafe { msg_send![self.window, frame] };
        frame.origin.y += frame.size.height;
        frame.origin.y -= new_height as f64;
        frame.size = NSSize {
            width: new_width as f64,
            height: new_height as f64,
        };
        let () =
            unsafe { msg_send![self.window, setFrame:frame display:true animate:true] };
    }
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        if self.fullscreen != fullscreen {
            self.fullscreen = fullscreen;
            unsafe {
                let () = msg_send![self.window, toggleFullScreen: nil];
            }
        }
    }
}

impl MacosDisplay {
    fn transform_mouse_point(&self, point: &NSPoint) -> (f32, f32) {
        let new_x = point.x as f32 * self.dpi_scale;
        let new_y = self.screen_height as f32 - (point.y as f32 * self.dpi_scale) - 1.;

        (new_x, new_y)
    }

    fn move_mouse_inside_window(&self, _window: *mut Object) {
        unsafe {
            let frame: NSRect = msg_send![self.window, frame];
            let origin = self.transform_mouse_point(&frame.origin);
            let point = NSPoint {
                x: (origin.0 as f64) + (frame.size.width / 2.0),
                y: (origin.1 as f64) + (frame.size.height / 2.0),
            };
            CGWarpMouseCursorPosition(point);
        }
    }

    unsafe fn update_dimensions(&mut self) -> Option<(i32, i32, f32)> {
        let screen: ObjcId = msg_send![self.window, screen];
        let dpi_scale: f64 = msg_send![screen, backingScaleFactor];
        self.dpi_scale = dpi_scale as f32;

        let bounds: NSRect = msg_send![self.view, bounds];
        let screen_width = (bounds.size.width as f32 * self.dpi_scale) as i32;
        let screen_height = (bounds.size.height as f32 * self.dpi_scale) as i32;

        let dim_changed =
            screen_width != self.screen_width || screen_height != self.screen_height;

        self.screen_width = screen_width;
        self.screen_height = screen_height;

        if dim_changed {
            Some((screen_width, screen_height, self.dpi_scale))
        } else {
            None
        }
    }
}

#[derive(Default)]
struct Modifiers {
    left_shift: bool,
    right_shift: bool,
    left_control: bool,
    right_control: bool,
    left_alt: bool,
    right_alt: bool,
    left_command: bool,
    right_command: bool,
}

impl Modifiers {
    const NS_RIGHT_SHIFT_KEY_MASK: u64 = 0x020004;
    const NS_LEFT_SHIFT_KEY_MASK: u64 = 0x020002;
    const NS_RIGHT_COMMAND_KEY_MASK: u64 = 0x100010;
    const NS_LEFT_COMMAND_KEY_MASK: u64 = 0x100008;
    const NS_RIGHT_ALTERNATE_KEY_MASK: u64 = 0x080040;
    const NS_LEFT_ALTERNATE_KEY_MASK: u64 = 0x080020;
    const NS_RIGHT_CONTROL_KEY_MASK: u64 = 0x042000;
    const NS_LEFT_CONTROL_KEY_MASK: u64 = 0x040001;

    pub fn new(flags: u64) -> Self {
        Self {
            left_shift: flags & Self::NS_LEFT_SHIFT_KEY_MASK
                == Self::NS_LEFT_SHIFT_KEY_MASK,
            right_shift: flags & Self::NS_RIGHT_SHIFT_KEY_MASK
                == Self::NS_RIGHT_SHIFT_KEY_MASK,
            left_alt: flags & Self::NS_LEFT_ALTERNATE_KEY_MASK
                == Self::NS_LEFT_ALTERNATE_KEY_MASK,
            right_alt: flags & Self::NS_RIGHT_ALTERNATE_KEY_MASK
                == Self::NS_RIGHT_ALTERNATE_KEY_MASK,
            left_control: flags & Self::NS_LEFT_CONTROL_KEY_MASK
                == Self::NS_LEFT_CONTROL_KEY_MASK,
            right_control: flags & Self::NS_RIGHT_CONTROL_KEY_MASK
                == Self::NS_RIGHT_CONTROL_KEY_MASK,
            left_command: flags & Self::NS_LEFT_COMMAND_KEY_MASK
                == Self::NS_LEFT_COMMAND_KEY_MASK,
            right_command: flags & Self::NS_RIGHT_COMMAND_KEY_MASK
                == Self::NS_RIGHT_COMMAND_KEY_MASK,
        }
    }
}

extern "C" fn rio_perform_key_assignment(
    _self: &Object,
    _sel: Sel,
    menu_item: *mut Object,
) {
    let menu_item = MenuItem::with_menu_item(menu_item);
    // Safe because waPerformKeyAssignment: is only used with KeyAssignment
    let opt_action = menu_item.get_represented_item();
    log::debug!("rio_perform_key_assignment {opt_action:?}",);
    if let Some(action) = opt_action {
        match action {
            RepresentedItem::KeyAssignment(KeyAssignment::SpawnWindow) => {
                App::create_window();
            }
            RepresentedItem::KeyAssignment(KeyAssignment::SpawnTab) => {
                App::create_tab(None);
            }
            RepresentedItem::KeyAssignment(KeyAssignment::Copy(ref text)) => {
                App::clipboard_set(text);
            }
        }
    }
}

extern "C" fn application_dock_menu(
    _self: &Object,
    _sel: Sel,
    _app: *mut Object,
) -> *mut Object {
    let dock_menu = Menu::new_with_title("");
    let new_window_item =
        MenuItem::new_with("New Window", Some(sel!(rioPerformKeyAssignment:)), "");
    new_window_item
        .set_represented_item(RepresentedItem::KeyAssignment(KeyAssignment::SpawnWindow));
    dock_menu.add_item(&new_window_item);
    dock_menu.autorelease()
}

extern "C" fn application_did_finish_launching(
    this: &mut Object,
    _sel: Sel,
    _notif: *mut Object,
) {
    log::debug!("application_did_finish_launching");
    unsafe {
        let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];
        (*this).set_ivar("launched", YES);

        if let Some(app) = NATIVE_APP.get() {
            let delegate = &**app.app_delegate;
            if let Some(app_state) = get_app_state(delegate) {
                app_state.waker.borrow_mut().start();
            }
        }

        let _: () = msg_send![pool, release];
    }
}

extern "C" fn application_open_urls(
    _this: &mut Object,
    _sel: Sel,
    _sender: ObjcId,
    urls: ObjcId,
) {
    let count: u64 = unsafe { msg_send![urls, count] };
    if count > 0 {
        let urls_to_send = {
            (0..count)
                .map(|index| {
                    let url: ObjcId = unsafe { msg_send![urls, objectAtIndex: index] };
                    let url: ObjcId = unsafe { msg_send![url, absoluteString] };
                    nsstring_to_string(url)
                })
                .collect::<Vec<String>>()
        };

        if urls_to_send.is_empty() {
            return;
        }

        for url in urls_to_send {
            App::create_tab(Some(&url));
        }
    }
}

#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NSApplicationTerminateReply {
    NSTerminateCancel = 0,
    NSTerminateNow = 1,
    NSTerminateLater = 2,
}

pub fn define_app_delegate() -> *const Class {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("NSAppDelegate", superclass).unwrap();

    extern "C" fn application_should_terminate(
        _self: &mut Object,
        _sel: Sel,
        _app: *mut Object,
    ) -> u64 {
        unsafe {
            let panel: *mut Object = msg_send![class!(NSAlert), new];

            let prompt = "All sessions will be closed";
            let title = "Quit Rio terminal?";
            let yes = "Yes";
            let no = "No";
            let cancel = "Cancel";

            let prompt_string: *mut Object = msg_send![class!(NSString), alloc];
            let prompt_allocated_string: *mut Object = msg_send![prompt_string, initWithBytes:prompt.as_ptr() length:prompt.len() encoding:4];

            let title_string: *mut Object = msg_send![class!(NSString), alloc];
            let title_allocated_string: *mut Object = msg_send![title_string, initWithBytes:title.as_ptr() length:title.len() encoding:4];

            let yes_string: *mut Object = msg_send![class!(NSString), alloc];
            let yes_allocated_string: *mut Object = msg_send![yes_string, initWithBytes:yes.as_ptr() length:yes.len() encoding:4];

            let no_string: *mut Object = msg_send![class!(NSString), alloc];
            let no_allocated_string: *mut Object = msg_send![no_string, initWithBytes:no.as_ptr() length:no.len() encoding:4];

            let cancel_string: *mut Object = msg_send![class!(NSString), alloc];
            let cancel_allocated_string: *mut Object = msg_send![cancel_string, initWithBytes:cancel.as_ptr() length:cancel.len() encoding:4];

            let _: () = msg_send![panel, setMessageText: title_allocated_string];
            let _: () = msg_send![panel, setInformativeText: prompt_allocated_string];
            let _: () = msg_send![panel, addButtonWithTitle: yes_allocated_string];
            let _: () = msg_send![panel, addButtonWithTitle: no_allocated_string];
            let _: () = msg_send![panel, addButtonWithTitle: cancel_allocated_string];
            let response: std::ffi::c_long = msg_send![panel, runModal];
            match response {
                1000 => NSApplicationTerminateReply::NSTerminateNow as u64,
                1001 => NSApplicationTerminateReply::NSTerminateCancel as u64,
                _ => NSApplicationTerminateReply::NSTerminateCancel as u64,
            }
        }
    }

    unsafe {
        decl.add_method(
            sel!(applicationDockMenu:),
            application_dock_menu
                as extern "C" fn(&Object, Sel, *mut Object) -> *mut Object,
        );
        decl.add_method(
            sel!(applicationShouldTerminate:),
            application_should_terminate
                as extern "C" fn(&mut Object, Sel, *mut Object) -> u64,
        );
        decl.add_method(
            sel!(applicationShouldTerminateAfterLastWindowClosed:),
            no1 as extern "C" fn(&Object, Sel, ObjcId) -> BOOL,
        );
        decl.add_method(
            sel!(applicationDidFinishLaunching:),
            application_did_finish_launching
                as extern "C" fn(&mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(rioPerformKeyAssignment:),
            rio_perform_key_assignment as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(application:openURLs:),
            application_open_urls as extern "C" fn(&mut Object, Sel, ObjcId, ObjcId),
        );
    }

    decl.add_ivar::<BOOL>("launched");
    decl.add_ivar::<*mut c_void>(VIEW_IVAR_NAME);
    decl.add_ivar::<*mut c_void>(APP_STATE_IVAR_NAME);

    decl.register()
}

#[inline]
fn send_resize_event(payload: &mut MacosDisplay, rescale: bool) {
    unsafe {
        if let Some((w, h, scale_factor)) = payload.update_dimensions() {
            if let Some(app_state) = get_app_state(&*payload.app) {
                app_state
                    .pending_events
                    .borrow_mut()
                    .push_back(QueuedEvent::Window(
                        payload.id,
                        WindowEvent::Resize(w, h, scale_factor, rescale),
                    ));
            }
        }
    }
}

#[inline]
unsafe fn view_base_decl(decl: &mut ClassDecl) {
    extern "C" fn mouse_moved(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("mouse_moved");

        if let Some(payload) = get_display_payload(this) {
            unsafe {
                if payload.cursor_grabbed {
                    // let dx: f64 = msg_send!(event, deltaX);
                    // let dy: f64 = msg_send!(event, deltaY);
                    // if let Ok(mut event_handler) = payload.event_handler.try_borrow_mut() {
                    //     event_handler.raw_mouse_motion(payload.id, dx as f32, dy as f32);
                    // }
                } else {
                    let point: NSPoint = msg_send!(event, locationInWindow);
                    let point = payload.transform_mouse_point(&point);

                    // Point is outside of view
                    if point.0.is_sign_negative()
                        || point.1.is_sign_negative()
                        || point.0 > payload.screen_width as f32
                        || point.1 > payload.screen_height as f32
                    {
                        return;
                    }

                    if let Some(app_state) = get_app_state(&*payload.app) {
                        app_state.pending_events.borrow_mut().push_back(
                            QueuedEvent::Window(
                                payload.id,
                                WindowEvent::MouseMotion(point.0, point.1),
                            ),
                        );
                    }
                }
            }
        }
    }

    fn fire_mouse_event(this: &Object, event: ObjcId, down: bool, btn: MouseButton) {
        log::info!("fire_mouse_event");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                let point: NSPoint = msg_send!(event, locationInWindow);
                let point = payload.transform_mouse_point(&point);
                if down {
                    if let Some(&mut HandlerState::Running {
                        ref mut handler, ..
                    }) = get_app_handler(&Some(payload.app))
                    {
                        handler
                            .mouse_button_down_event(payload.id, btn, point.0, point.1);
                    }

                    // if let Ok(mut event_handler) = payload.event_handler.try_borrow_mut()
                    // {
                    //     event_handler
                    //         .mouse_button_down_event(payload.id, btn, point.0, point.1);
                    // }
                } else if let Some(&mut HandlerState::Running {
                    ref mut handler, ..
                }) = get_app_handler(&Some(payload.app))
                {
                    handler.mouse_button_up_event(payload.id, btn, point.0, point.1);
                }
            }
        }
    }

    extern "C" fn do_command_by_selector(this: &Object, _sel: Sel, _a_selector: Sel) {
        log::info!("do_command_by_selector");
        if let Some(payload) = get_display_payload(this) {
            if payload.ime == ImeState::Commited {
                return;
            }

            if !payload.marked_text.is_empty() && payload.ime == ImeState::Preedit {
                payload.ime = ImeState::Ground;
            }
        }
    }

    extern "C" fn has_marked_text(this: &Object, _sel: Sel) -> BOOL {
        log::info!("has_marked_text");
        if let Some(payload) = get_display_payload(this) {
            if !payload.marked_text.is_empty() {
                YES
            } else {
                NO
            }
        } else {
            NO
        }
    }

    extern "C" fn marked_range(this: &Object, _sel: Sel) -> NSRange {
        log::info!("marked_range");
        if let Some(payload) = get_display_payload(this) {
            if !payload.marked_text.is_empty() {
                NSRange::new(0, payload.marked_text.len() as u64)
            } else {
                NSRange::new(NSNOT_FOUND as _, 0)
            }
        } else {
            NSRange::new(NSNOT_FOUND as _, 0)
        }
    }

    extern "C" fn selected_range(_this: &Object, _sel: Sel) -> NSRange {
        log::info!("selected_range");
        NSRange {
            location: 0,
            length: 1,
        }
    }

    // Called by the IME when inserting composed text and/or emoji
    extern "C" fn insert_text_replacement_range(
        this: &Object,
        _sel: Sel,
        astring: ObjcId,
        _replacement_range: NSRange,
    ) {
        log::info!("insert_text_replacement_range");
        let string = nsstring_to_string(astring);
        let is_control = string.chars().next().map_or(false, |c| c.is_control());

        if !is_control {
            if let Some(payload) = get_display_payload(this) {
                // Commit only if we have marked text.
                if !payload.marked_text.is_empty() && payload.ime != ImeState::Disabled {
                    if let Some(&mut HandlerState::Running {
                        ref mut handler, ..
                    }) = get_app_handler(&Some(payload.app))
                    {
                        handler.ime_event(
                            payload.id,
                            crate::ImeState::Preedit(String::new(), None),
                        );
                        handler.ime_event(payload.id, crate::ImeState::Commit(string));
                        payload.ime = ImeState::Commited;
                    }
                }
            }

            // unsafe {
            //     let input_context: ObjcId = msg_send![this, inputContext];
            //     let () = msg_send![input_context, invalidateCharacterCoordinates];
            // }
        }
    }

    extern "C" fn set_marked_text_selected_range_replacement_range(
        this: &Object,
        _sel: Sel,
        astring: ObjcId,
        _selected_range: NSRange,
        _replacement_range: NSRange,
    ) {
        log::info!("set_marked_text_selected_range_replacement_range");
        let s = nsstring_to_string(astring);

        if let Some(payload) = get_display_payload(this) {
            let preedit_string: String = s.to_string();
            payload.marked_text.clone_from(&preedit_string);

            if payload.ime == ImeState::Disabled {
                if let Some(&mut HandlerState::Running {
                    ref mut handler, ..
                }) = get_app_handler(&Some(payload.app))
                {
                    handler.ime_event(payload.id, crate::ImeState::Enabled);
                }
            }

            if !payload.marked_text.is_empty() {
                payload.ime = ImeState::Preedit;
            } else {
                // In case the preedit was cleared, set IME into the Ground state.
                payload.ime = ImeState::Ground;
            }

            let cursor_range = if payload.marked_text.is_empty() {
                None
            } else {
                Some((payload.marked_text.len(), payload.marked_text.len()))
            };

            if let Some(&mut HandlerState::Running {
                ref mut handler, ..
            }) = get_app_handler(&Some(payload.app))
            {
                handler.ime_event(
                    payload.id,
                    crate::ImeState::Preedit(preedit_string, cursor_range),
                );
            }
        }
    }

    extern "C" fn unmark_text(this: &Object, _sel: Sel) {
        log::info!("unmark_text");
        if let Some(payload) = get_display_payload(this) {
            payload.marked_text.clear();
            payload.ime = ImeState::Ground;

            // unsafe {
            //     let input_context: ObjcId = msg_send![this, inputContext];
            //     let _: () = msg_send![input_context, discardMarkedText];
            // }
        }
    }

    extern "C" fn character_index_for_point(
        _this: &Object,
        _sel: Sel,
        _point: NSPoint,
    ) -> NSUInteger {
        log::info!("character_index_for_point");
        NSNOT_FOUND as _
    }

    extern "C" fn first_rect_for_character_range(
        this: &Object,
        _sel: Sel,
        _range: NSRange,
        _actual: *mut c_void,
    ) -> NSRect {
        log::info!("first_rect_for_character_range");

        // Returns a rect in screen coordinates; this is used to place
        // the input method editor
        let window: ObjcId = unsafe { msg_send![this, window] };
        let frame: NSRect = unsafe { msg_send![window, frame] };

        let content: NSRect =
            unsafe { msg_send![window, contentRectForFrameRect: frame] };
        // let backing_frame: NSRect =
        //     unsafe { msg_send![this, convertRectToBacking: frame] };

        if let Some(payload) = get_display_payload(this) {
            let point: NSPoint = unsafe { msg_send!(this, locationInWindow) };
            let cursor_pos = payload.transform_mouse_point(&point);

            NSRect {
                origin: NSPoint {
                    x: content.origin.x + (cursor_pos.0 as f64),
                    y: content.origin.y + content.size.height - (cursor_pos.1 as f64),
                },
                size: NSSize {
                    width: cursor_pos.0 as f64,
                    height: cursor_pos.1 as f64,
                },
            }
        } else {
            frame
        }
    }

    extern "C" fn valid_attributes_for_marked_text(_this: &Object, _sel: Sel) -> ObjcId {
        log::info!("valid_attributes_for_marked_text");
        // FIXME: returns NSArray<NSAttributedStringKey> *
        let content: &[ObjcId; 0] = &[];
        unsafe {
            msg_send![class!(NSArray),
                arrayWithObjects: content.as_ptr()
                count: content.len()
            ]
        }
    }

    extern "C" fn attributed_substring_for_proposed_range(
        _this: &Object,
        _sel: Sel,
        _proposed_range: NSRange,
        _actual_range: *mut c_void,
    ) -> ObjcId {
        nil
    }

    extern "C" fn mouse_down(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("mouse_down");
        fire_mouse_event(this, event, true, MouseButton::Left);
    }
    extern "C" fn mouse_up(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("mouse_up");
        fire_mouse_event(this, event, false, MouseButton::Left);
    }
    extern "C" fn right_mouse_down(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("right_mouse_down");
        fire_mouse_event(this, event, true, MouseButton::Right);
    }
    extern "C" fn right_mouse_up(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("right_mouse_up");
        fire_mouse_event(this, event, false, MouseButton::Right);
    }
    extern "C" fn other_mouse_down(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("other_mouse_down");
        fire_mouse_event(this, event, true, MouseButton::Middle);
    }
    extern "C" fn other_mouse_up(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("other_mouse_up");
        fire_mouse_event(this, event, false, MouseButton::Middle);
    }
    extern "C" fn scroll_wheel(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("scroll_wheel");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                let mut dx: f64 = msg_send![event, scrollingDeltaX];
                let mut dy: f64 = msg_send![event, scrollingDeltaY];

                if !msg_send![event, hasPreciseScrollingDeltas] {
                    dx *= 10.0;
                    dy *= 10.0;
                }

                if let Some(&mut HandlerState::Running {
                    ref mut handler, ..
                }) = get_app_handler(&Some(payload.app))
                {
                    handler.mouse_wheel_event(payload.id, dx as f32, dy as f32);
                }
            }
        }
    }
    extern "C" fn window_did_become_key(this: &Object, _sel: Sel, _event: ObjcId) {
        log::info!("window_did_become_key");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                if let Some(app_state) = get_app_state(&*payload.app) {
                    app_state
                        .pending_events
                        .borrow_mut()
                        .push_back(QueuedEvent::Window(
                            payload.id,
                            WindowEvent::Focus(true),
                        ));
                }
            }
            payload.has_focus = true;
        }
    }
    extern "C" fn window_did_resign_key(this: &Object, _sel: Sel, _event: ObjcId) {
        log::info!("window_did_resign_key");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                if let Some(app_state) = get_app_state(&*payload.app) {
                    app_state
                        .pending_events
                        .borrow_mut()
                        .push_back(QueuedEvent::Window(
                            payload.id,
                            WindowEvent::Focus(false),
                        ));
                }
            }
            payload.has_focus = false;
        }
    }
    extern "C" fn reset_cursor_rects(this: &Object, _sel: Sel) {
        log::info!("reset_cursor_rects");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                let cursor_id = {
                    let current_cursor = payload.current_cursor;
                    let cursor_id = *payload
                        .cursors
                        .entry(current_cursor)
                        .or_insert_with(|| load_mouse_cursor(current_cursor));
                    assert!(!cursor_id.is_null());
                    cursor_id
                };

                let bounds: NSRect = msg_send![this, bounds];
                let _: () = msg_send![
                    this,
                    addCursorRect: bounds
                    cursor: cursor_id
                ];
            }
        }
    }

    extern "C" fn dragging_entered(this: &Object, _: Sel, sender: ObjcId) -> BOOL {
        log::info!("dragging_entered");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                let pboard: ObjcId = msg_send![sender, draggingPasteboard];
                let filenames: ObjcId =
                    msg_send![pboard, propertyListForType: NSFilenamesPboardType];
                let count: u64 = msg_send![filenames, count];
                if count > 0 {
                    let mut dragged_files = vec![];
                    for index in 0..count {
                        let item = msg_send![filenames, objectAtIndex: index];
                        let path = nsstring_to_string(item);
                        dragged_files.push(std::path::PathBuf::from(path));
                    }

                    if let Some(&mut HandlerState::Running {
                        ref mut handler, ..
                    }) = get_app_handler(&Some(payload.app))
                    {
                        handler.files_dragged_event(
                            payload.id,
                            dragged_files,
                            crate::DragState::Entered,
                        );
                    }
                }
            }
        }
        YES
    }

    extern "C" fn dragging_exited(this: &Object, _: Sel, _sender: ObjcId) {
        log::info!("dragging_exited");
        if let Some(payload) = get_display_payload(this) {
            if let Some(&mut HandlerState::Running {
                ref mut handler, ..
            }) = get_app_handler(&Some(payload.app))
            {
                handler.files_dragged_event(payload.id, vec![], crate::DragState::Exited);
            }
        }
    }

    extern "C" fn perform_drag_operation(this: &Object, _: Sel, sender: ObjcId) -> BOOL {
        log::info!("perform_drag_operation");
        if let Some(payload) = get_display_payload(this) {
            unsafe {
                let pboard: ObjcId = msg_send![sender, draggingPasteboard];
                let filenames: ObjcId =
                    msg_send![pboard, propertyListForType: NSFilenamesPboardType];
                let count: u64 = msg_send![filenames, count];
                if count > 0 {
                    let mut dropped_files = vec![];
                    for index in 0..count {
                        let item = msg_send![filenames, objectAtIndex: index];
                        let path = nsstring_to_string(item);
                        dropped_files.push(std::path::PathBuf::from(path));
                    }

                    if let Some(&mut HandlerState::Running {
                        ref mut handler, ..
                    }) = get_app_handler(&Some(payload.app))
                    {
                        handler.files_dropped_event(payload.id, dropped_files);
                    }
                }
            }
        }
        YES
    }

    extern "C" fn window_should_close(this: &Object, _: Sel, _: ObjcId) -> BOOL {
        log::info!("window_should_close");
        let payload = get_display_payload(this);

        if payload.is_none() {
            return NO;
        }

        let payload = payload.unwrap();

        unsafe {
            let capture_manager =
                msg_send_![class![MTLCaptureManager], sharedCaptureManager];
            msg_send_![capture_manager, stopCapture];
        }

        // only give user-code a chance to intervene when sapp_quit() wasn't already called
        if !get_handler().lock().get(payload.id).unwrap().quit_ordered {
            // if window should be closed and event handling is enabled, give user code
            // a chance to intervene via sapp_cancel_quit()
            get_handler()
                .lock()
                .get_mut(payload.id)
                .unwrap()
                .quit_requested = true;

            if let Some(&mut HandlerState::Running {
                ref mut handler, ..
            }) = get_app_handler(&Some(payload.app))
            {
                handler.quit_requested_event();
            }

            // user code hasn't intervened, quit the app
            if get_handler().lock().get(payload.id).unwrap().quit_requested {
                get_handler()
                    .lock()
                    .get_mut(payload.id)
                    .unwrap()
                    .quit_ordered = true;
            }
        }
        if get_handler().lock().get(payload.id).unwrap().quit_ordered {
            YES
        } else {
            NO
        }
    }

    extern "C" fn window_did_resize(this: &Object, _: Sel, _: ObjcId) {
        log::info!("window_did_resize");
        if let Some(payload) = get_display_payload(this) {
            send_resize_event(payload, false);
        }
    }
    extern "C" fn window_did_change_screen(this: &Object, _: Sel, _: ObjcId) {
        log::info!("window_did_change_screen");
        if let Some(payload) = get_display_payload(this) {
            send_resize_event(payload, true);
        }
    }
    extern "C" fn window_did_enter_fullscreen(this: &Object, _: Sel, _: ObjcId) {
        log::info!("window_did_enter_fullscreen");
        if let Some(payload) = get_display_payload(this) {
            payload.fullscreen = true;
        }
    }
    extern "C" fn window_did_exit_fullscreen(this: &Object, _: Sel, _: ObjcId) {
        log::info!("window_did_exit_fullscreen");
        if let Some(payload) = get_display_payload(this) {
            payload.fullscreen = false;
        }
    }

    extern "C" fn key_down(this: &Object, _sel: Sel, event: ObjcId) {
        log::info!("key_down");
        if let Some(payload) = get_display_payload(this) {
            let repeat: bool = unsafe { msg_send!(event, isARepeat) };
            let unmod = unsafe { msg_send!(event, charactersIgnoringModifiers) };
            let unmod = nsstring_to_string(unmod);
            let chars = get_event_char(event);

            log::info!("KEY_DOWN (chars={:?} unmod={:?}", chars, unmod,);

            let old_ime = &payload.ime;
            // unmod is differently depending of the keymap used, for example if you
            // are using US-International and press CTRL -> Key N -> Key Space will produce `~`.
            if unmod.is_empty() || !repeat {
                unsafe {
                    let input_context: ObjcId = msg_send![this, inputContext];
                    let _res: BOOL = msg_send![input_context, handleEvent: event];
                    // if res == YES {
                    //     return;
                    // }
                }
            }

            let had_ime_input = match payload.ime {
                ImeState::Commited => {
                    // Allow normal input after the commit.
                    payload.ime = ImeState::Ground;
                    payload.marked_text = String::from("");
                    true
                }
                ImeState::Preedit => true,
                // `key_down` could result in preedit clear, so compare old and current state.
                _ => old_ime != &payload.ime,
            };

            if !had_ime_input {
                if let Some(key) = get_event_keycode(event) {
                    // if let Some(event_handler) = payload.context() {

                    if let Some(&mut HandlerState::Running {
                        ref mut handler, ..
                    }) = get_app_handler(&Some(payload.app))
                    {
                        handler.key_down_event(
                            payload.id,
                            key,
                            repeat,
                            get_event_char(event),
                        );
                    }

                    // }
                }
            }
        }
    }

    extern "C" fn appearance_did_change(this: &Object, _sel: Sel, _app: ObjcId) {
        log::info!("appearance_did_change");
        if let Some(payload) = get_display_payload(this) {
            if let Some(&mut HandlerState::Running {
                ref mut handler, ..
            }) = get_app_handler(&Some(payload.app))
            {
                handler.appearance_change_event(payload.id, App::appearance());
            }
        }
    }

    extern "C" fn key_up(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_display_payload(this) {
            if let Some(key) = get_event_keycode(event) {
                log::info!("KEY_UP (key={:?}", key);
                if let Some(&mut HandlerState::Running {
                    ref mut handler, ..
                }) = get_app_handler(&Some(payload.app))
                {
                    handler.key_up_event(payload.id, key);
                }
            }
        }
    }

    extern "C" fn flags_changed(this: &Object, _sel: Sel, event: ObjcId) {
        fn produce_event(
            payload: &mut MacosDisplay,
            keycode: crate::KeyCode,
            mods: crate::ModifiersState,
            old_pressed: bool,
            new_pressed: bool,
        ) {
            if new_pressed ^ old_pressed {
                if new_pressed {
                    if let Some(&mut HandlerState::Running {
                        ref mut handler, ..
                    }) = get_app_handler(&Some(payload.app))
                    {
                        handler.modifiers_event(payload.id, Some(keycode), mods);
                    }
                } else if let Some(&mut HandlerState::Running {
                    ref mut handler, ..
                }) = get_app_handler(&Some(payload.app))
                {
                    handler.modifiers_event(payload.id, None, mods);
                }
            }
        }

        if let Some(payload) = get_display_payload(this) {
            let mods = get_event_key_modifier(event);
            let flags: u64 = unsafe { msg_send![event, modifierFlags] };
            let new_modifiers = Modifiers::new(flags);

            produce_event(
                payload,
                crate::KeyCode::LeftShift,
                mods,
                payload.modifiers.left_shift,
                new_modifiers.left_shift,
            );
            produce_event(
                payload,
                crate::KeyCode::RightShift,
                mods,
                payload.modifiers.right_shift,
                new_modifiers.right_shift,
            );
            produce_event(
                payload,
                crate::KeyCode::LeftControl,
                mods,
                payload.modifiers.left_control,
                new_modifiers.left_control,
            );
            produce_event(
                payload,
                crate::KeyCode::RightControl,
                mods,
                payload.modifiers.right_control,
                new_modifiers.right_control,
            );
            produce_event(
                payload,
                crate::KeyCode::LeftSuper,
                mods,
                payload.modifiers.left_command,
                new_modifiers.left_command,
            );
            produce_event(
                payload,
                crate::KeyCode::RightSuper,
                mods,
                payload.modifiers.right_command,
                new_modifiers.right_command,
            );
            produce_event(
                payload,
                crate::KeyCode::LeftAlt,
                mods,
                payload.modifiers.left_alt,
                new_modifiers.left_alt,
            );
            produce_event(
                payload,
                crate::KeyCode::RightAlt,
                mods,
                payload.modifiers.right_alt,
                new_modifiers.right_alt,
            );

            payload.modifiers = new_modifiers;
        }
    }
    decl.add_method(
        sel!(canBecomeKey),
        yes as extern "C" fn(&Object, Sel) -> BOOL,
    );
    decl.add_method(
        sel!(acceptsFirstResponder),
        yes as extern "C" fn(&Object, Sel) -> BOOL,
    );
    decl.add_method(sel!(isOpaque), yes as extern "C" fn(&Object, Sel) -> BOOL);
    decl.add_method(
        sel!(resetCursorRects),
        reset_cursor_rects as extern "C" fn(&Object, Sel),
    );
    decl.add_method(
        sel!(mouseMoved:),
        mouse_moved as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(mouseDragged:),
        mouse_moved as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(rightMouseDragged:),
        mouse_moved as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(otherMouseDragged:),
        mouse_moved as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(mouseDown:),
        mouse_down as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(mouseUp:),
        mouse_up as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(rightMouseDown:),
        right_mouse_down as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(rightMouseUp:),
        right_mouse_up as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(otherMouseDown:),
        other_mouse_down as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(otherMouseUp:),
        other_mouse_up as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(scrollWheel:),
        scroll_wheel as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(windowDidBecomeKey:),
        window_did_become_key as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(windowDidResignKey:),
        window_did_resign_key as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(flagsChanged:),
        flags_changed as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(windowShouldClose:),
        window_should_close as extern "C" fn(&Object, Sel, ObjcId) -> BOOL,
    );
    decl.add_method(
        sel!(windowDidResize:),
        window_did_resize as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(windowDidChangeScreen:),
        window_did_change_screen as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(windowDidEnterFullScreen:),
        window_did_enter_fullscreen as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(windowDidExitFullScreen:),
        window_did_exit_fullscreen as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(sel!(keyUp:), key_up as extern "C" fn(&Object, Sel, ObjcId));
    decl.add_method(
        sel!(keyDown:),
        key_down as extern "C" fn(&Object, Sel, ObjcId),
    );

    decl.add_method(
        sel!(draggingEntered:),
        dragging_entered as extern "C" fn(&Object, Sel, ObjcId) -> BOOL,
    );
    decl.add_method(
        sel!(draggingExited:),
        dragging_exited as extern "C" fn(&Object, Sel, ObjcId),
    );
    decl.add_method(
        sel!(performDragOperation:),
        perform_drag_operation as extern "C" fn(&Object, Sel, ObjcId) -> BOOL,
    );

    // NSTextInputClient
    decl.add_method(
        sel!(doCommandBySelector:),
        do_command_by_selector as extern "C" fn(&Object, Sel, Sel),
    );
    decl.add_method(
        sel!(characterIndexForPoint:),
        character_index_for_point as extern "C" fn(&Object, Sel, NSPoint) -> NSUInteger,
    );
    decl.add_method(
        sel!(firstRectForCharacterRange:actualRange:),
        first_rect_for_character_range
            as extern "C" fn(&Object, Sel, NSRange, *mut c_void) -> NSRect,
    );
    decl.add_method(
        sel!(hasMarkedText),
        has_marked_text as extern "C" fn(&Object, Sel) -> BOOL,
    );
    decl.add_method(
        sel!(markedRange),
        marked_range as extern "C" fn(&Object, Sel) -> NSRange,
    );
    decl.add_method(
        sel!(selectedRange),
        selected_range as extern "C" fn(&Object, Sel) -> NSRange,
    );
    decl.add_method(
        sel!(setMarkedText:selectedRange:replacementRange:),
        set_marked_text_selected_range_replacement_range
            as extern "C" fn(&Object, Sel, ObjcId, NSRange, NSRange),
    );
    decl.add_method(sel!(unmarkText), unmark_text as extern "C" fn(&Object, Sel));
    decl.add_method(
        sel!(validAttributesForMarkedText),
        valid_attributes_for_marked_text as extern "C" fn(&Object, Sel) -> ObjcId,
    );
    decl.add_method(
        sel!(attributedSubstringForProposedRange:actualRange:),
        attributed_substring_for_proposed_range
            as extern "C" fn(&Object, Sel, NSRange, *mut c_void) -> ObjcId,
    );
    decl.add_method(
        sel!(insertText:replacementRange:),
        insert_text_replacement_range as extern "C" fn(&Object, Sel, ObjcId, NSRange),
    );
    // Appearence
    decl.add_method(
        sel!(appearanceDidChange:),
        appearance_did_change as extern "C" fn(&Object, Sel, ObjcId),
    );

    // TODO:
    // When keyboard changes should drop IME
    // #[method_id(selectedKeyboardInputSource)]
    // pub fn selectedKeyboardInputSource(&self) -> Option<Id<NSTextInputSourceIdentifier>>;
}

#[inline]
extern "C" fn draw_rect(this: &Object, _sel: Sel, _rect: NSRect) {
    log::info!("draw_rect");
    if let Some(payload) = get_display_payload(this) {
        if !payload.has_initialized {
            unsafe { payload.update_dimensions() };

            if let Some(&mut HandlerState::Running {
                ref mut handler, ..
            }) = get_app_handler(&Some(payload.app))
            {
                handler.resize_event(
                    payload.id,
                    payload.screen_width,
                    payload.screen_height,
                    payload.dpi_scale,
                    true,
                );
                payload.has_initialized = true;
            }
        }
    }
}

#[inline]
fn initialize_view(this: &Object) -> (i32, i32, f32) {
    if let Some(payload) = get_display_payload(this) {
        unsafe { payload.update_dimensions() };

        return (
            payload.screen_width,
            payload.screen_height,
            payload.dpi_scale,
        );
    }

    // TODO: Use constants for defaults
    (800, 600, 1.0)
}

pub fn define_metal_view_class(
    target: crate::Target,
    view_class_name: &str,
) -> *const Class {
    let superclass = match target {
        crate::Target::Application => class!(NSView),
        crate::Target::Game => class!(MTKView),
    };
    let mut decl = ClassDecl::new(view_class_name, superclass).unwrap();

    extern "C" fn display_layer(this: &mut Object, sel: Sel, _layer_id: ObjcId) {
        let rect = NSRect {
            origin: NSPoint { x: 0.0, y: 0.0 },
            size: NSSize {
                width: 0.0,
                height: 0.0,
            },
        };

        draw_rect(this, sel, rect)
    }

    // extern "C" fn wants_update_layer(_view: &mut Object, _sel: Sel) -> BOOL {
    //     YES
    // }

    // extern "C" fn draw_layer_in_context(
    //     _view: &mut Object,
    //     _sel: Sel,
    //     _layer_id: ObjcId,
    //     _context: ObjcId,
    // ) {
    // }

    // extern "C" fn update_layer(_this: &mut Object, _sel: Sel) {
    //     log::trace!("update_layer called");
    // }

    // extern "C" fn timer_fired(_this: &Object, _sel: Sel, _: ObjcId) {
    // Information retired from https://github.com/rust-windowing/winit
    // `setNeedsDisplay` does nothing on UIViews which are directly backed by CAEAGLLayer or CAMetalLayer.
    // Ordinarily the OS sets up a bunch of UIKit state before calling drawRect: on a UIView, but when using
    // raw or gl/metal for drawing this work is completely avoided.
    //
    // The docs for `setNeedsDisplay` don't mention `CAMetalLayer`; however, this has been confirmed via
    // testing.
    //
    // https://developer.apple.com/documentation/uikit/uiview/1622437-setneedsdisplay?language=objc

    // unsafe {
    //     let () = msg_send!(this, setNeedsDisplay: YES);
    // }
    // }

    extern "C" fn dealloc(this: &Object, _sel: Sel) {
        // TODO: dealloc MTKView if target is game
        unsafe {
            let superclass = class!(NSView); // MTKView
            let () = msg_send![super(this, superclass), dealloc];
        }
    }

    extern "C" fn make_backing_layer(this: &mut Object, _: Sel) -> ObjcId {
        log::trace!("make_backing_layer");
        let class = class!(CAMetalLayer);
        unsafe {
            let layer: ObjcId = msg_send![class, new];
            let () = msg_send![layer, setDelegate: class];
            let () = msg_send![layer, setContentsScale: 1.0];
            let () = msg_send![layer, setOpaque: NO];
            if let Some(payload) = get_display_payload(this) {
                *payload.renderer = layer;
            }

            let mtl_device_obj = MTLCreateSystemDefaultDevice();
            let () = msg_send![layer, setDevice: mtl_device_obj];
            let () = msg_send![layer, setColorPixelFormat: MTLPixelFormat::BGRA8Unorm];
            let () = msg_send![
                layer,
                setDepthStencilPixelFormat: MTLPixelFormat::Depth32Float_Stencil8
            ];
            let () = msg_send![layer, setSampleCount: 1];
            let () = msg_send![layer, setWantsLayer: YES];
            layer
        }
    }

    unsafe {
        decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));
        // decl.add_method(
        //     sel!(timerFired:),
        //     timer_fired as extern "C" fn(&Object, Sel, ObjcId),
        // );
        // decl.add_method(
        //     sel!(drawRect:),
        //     draw_rect as extern "C" fn(&Object, Sel, NSRect),
        // );
        decl.add_method(
            sel!(displayLayer:),
            display_layer as extern "C" fn(&mut Object, Sel, ObjcId),
        );
        decl.add_method(
            sel!(makeBackingLayer),
            make_backing_layer as extern "C" fn(&mut Object, Sel) -> ObjcId,
        );
        // decl.add_method(
        //     sel!(updateLayer),
        //     update_layer as extern "C" fn(&mut Object, Sel),
        // );
        // decl.add_method(
        //     sel!(wantsUpdateLayer),
        //     wants_update_layer as extern "C" fn(&mut Object, Sel) -> BOOL,
        // );
        // decl.add_method(
        //     sel!(drawLayer:inContext:),
        //     draw_layer_in_context as extern "C" fn(&mut Object, Sel, ObjcId, ObjcId),
        // );

        view_base_decl(&mut decl);
    }

    decl.add_ivar::<*mut c_void>(VIEW_IVAR_NAME);
    decl.add_protocol(
        Protocol::get("NSTextInputClient")
            .expect("failed to get NSTextInputClient protocol"),
    );
    decl.add_protocol(
        Protocol::get("CALayerDelegate").expect("CALayerDelegate not defined"),
    );

    decl.register()
}

#[inline]
pub fn get_display_payload(this: &Object) -> Option<&mut MacosDisplay> {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(VIEW_IVAR_NAME);
        if ptr.is_null() {
            None
        } else {
            Some(&mut *(ptr as *mut MacosDisplay))
        }
    }
}

#[inline]
fn get_app_state(this: &Object) -> Option<&mut AppState> {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(APP_STATE_IVAR_NAME);
        if ptr.is_null() {
            None
        } else {
            Some(&mut *(ptr as *mut AppState))
        }
    }
}

#[inline]
fn get_app_handler(app: &Option<*mut Object>) -> Option<&mut HandlerState> {
    let delegate: *mut Object = if let Some(this) = app {
        *this
    } else if let Some(native_app) = NATIVE_APP.get() {
        *native_app.app_delegate
    } else {
        return None;
    };

    // TODO: Remove this unsafe
    unsafe {
        if let Some(app_state) = get_app_state(&*delegate) {
            return app_state.handler.as_mut();
        }
    }

    None
}

struct View {
    inner: StrongPtr,
}

impl View {
    unsafe fn create_metal_view(
        target: crate::Target,
        _: NSRect,
        sample_count: i32,
        class_name: &str,
    ) -> Self {
        let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];
        let view_class = define_metal_view_class(target, class_name);
        let view: ObjcId = msg_send![view_class, alloc];
        let view: StrongPtr = StrongPtr::new(msg_send![view, init]);

        match target {
            crate::Target::Game => {
                let mtl_device_obj = MTLCreateSystemDefaultDevice();
                let () = msg_send![*view, setDevice: mtl_device_obj];
                let () =
                    msg_send![*view, setColorPixelFormat: MTLPixelFormat::BGRA8Unorm];
                let () = msg_send![
                    *view,
                    setDepthStencilPixelFormat: MTLPixelFormat::Depth32Float_Stencil8
                ];
                let () = msg_send![*view, setSampleCount: sample_count];
            }
            crate::Target::Application => {
                let _: () = msg_send![
                    *view,
                    setLayerContentsRedrawPolicy: NSViewLayerContentsRedrawDuringViewResize
                ];
            }
        }

        let () = msg_send![pool, drain];

        Self { inner: view }
    }

    #[inline]
    pub fn as_strong_ptr(&self) -> &StrongPtr {
        &self.inner
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.inner.is_null()
    }
}

#[repr(usize)] // NSUInteger
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NSWindowButton {
    Close = 0,
    Miniaturize = 1,
    Zoom = 2,
    #[allow(unused)]
    Toolbar = 3,
    #[allow(unused)]
    DocumentIcon = 4,
    #[allow(unused)]
    DocumentVersions = 6,
    // Deprecated since macOS 10.12
    FullScreen = 7,
}

#[allow(dead_code)]
#[repr(isize)] // NSInteger
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NSWindowTabbingMode {
    NSWindowTabbingModeAutomatic = 0,
    NSWindowTabbingModeDisallowed = 2,
    NSWindowTabbingModePreferred = 1,
}

pub struct App {
    pub inner: StrongPtr,
    app_delegate: StrongPtr,
    target: crate::Target,
}

unsafe impl Send for App {}
unsafe impl Sync for App {}

impl App {
    pub fn new(target: crate::Target, f: Box<dyn EventHandler + 'static>) -> App {
        unsafe {
            let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];
            crate::set_handler();

            let app_delegate_class = define_app_delegate();
            let app_delegate_instance =
                StrongPtr::new(msg_send![app_delegate_class, new]);

            let ns_app =
                { StrongPtr::new(msg_send![class!(NSApplication), sharedApplication]) };

            // create AppState
            let boxed_state = AppState::new(HandlerState::Running { handler: f });
            let boxed_state = Box::into_raw(Box::new(boxed_state));
            (**app_delegate_instance).set_ivar(
                APP_STATE_IVAR_NAME,
                &mut *boxed_state as *mut _ as *mut c_void,
            );

            let () = msg_send![*ns_app, setDelegate: *app_delegate_instance];
            let () = msg_send![
                *ns_app,
                setActivationPolicy: NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular
                    as i64
            ];
            let () = msg_send![*ns_app, activateIgnoringOtherApps: YES];

            if target == crate::Target::Application {
                App::configure_observer();
            }

            let _: () = msg_send![pool, release];

            let _ = NATIVE_APP.set(App {
                inner: ns_app.clone(),
                app_delegate: app_delegate_instance.clone(),
                target,
            });
            App {
                inner: ns_app,
                app_delegate: app_delegate_instance,
                target,
            }
        }
    }

    pub fn create_window() {
        if let Some(&mut HandlerState::Running {
            ref mut handler, ..
        }) = get_app_handler(&None)
        {
            handler.create_window();
        }
    }

    pub fn create_tab(tab_payload: Option<&str>) {
        if let Some(&mut HandlerState::Running {
            ref mut handler, ..
        }) = get_app_handler(&None)
        {
            handler.create_tab(tab_payload);
        }
    }

    extern "C" fn trigger(
        _observer: *mut __CFRunLoopObserver,
        activity: CFRunLoopActivity,
        _repeats: *mut std::ffi::c_void,
    ) {
        match activity {
            #[allow(non_upper_case_globals)]
            kCFRunLoopAfterWaiting => {
                // println!("kCFRunLoopAfterWaiting");
                if let Some(app) = NATIVE_APP.get() {
                    let delegate = unsafe { &**app.app_delegate };
                    if let Some(app_state) = get_app_state(delegate) {
                        let events: VecDeque<QueuedEvent> =
                            std::mem::take(&mut *app_state.pending_events.borrow_mut());
                        for event in events {
                            match event {
                                QueuedEvent::Window(window_id, event) => match event {
                                    WindowEvent::Focus(focus) => {
                                        if let Some(HandlerState::Running {
                                            ref mut handler,
                                            ..
                                        }) = app_state.handler
                                        {
                                            handler.focus_event(window_id, focus);
                                        }
                                    }
                                    WindowEvent::MouseMotion(pos_x, pos_y) => {
                                        if let Some(HandlerState::Running {
                                            ref mut handler,
                                            ..
                                        }) = app_state.handler
                                        {
                                            handler.mouse_motion_event(
                                                window_id, pos_x, pos_y,
                                            );
                                        };
                                    }
                                    WindowEvent::Resize(
                                        width,
                                        height,
                                        scale_factor,
                                        rescale,
                                    ) => {
                                        if let Some(HandlerState::Running {
                                            ref mut handler,
                                            ..
                                        }) = app_state.handler
                                        {
                                            handler.resize_event(
                                                window_id,
                                                width,
                                                height,
                                                scale_factor,
                                                rescale,
                                            );
                                        };
                                    }
                                },
                            }
                        }

                        let control = match app_state.handler {
                            Some(HandlerState::Running {
                                ref mut handler, ..
                            }) => handler.process(),
                            _ => EventHandlerControl::Wait,
                        };

                        let app_timeout = match control {
                            EventHandlerControl::Wait => None,
                            EventHandlerControl::Running => {
                                Some(std::time::Instant::now())
                            }
                            EventHandlerControl::WaitUntil(instant) => Some(instant),
                        };
                        app_state.waker.borrow_mut().start_at(app_timeout);
                    }
                }
            }
            #[allow(non_upper_case_globals)]
            kCFRunLoopBeforeWaiting => {
                // println!("kCFRunLoopBeforeWaiting");
            }
            #[allow(non_upper_case_globals)]
            kCFRunLoopExit => {
                // println!("kCFRunLoopExit");
            }
            _ => {}
        }

        // let _: () = msg_send![pool, release];
    }

    fn configure_observer() {
        unsafe {
            let observer = CFRunLoopObserverCreate(
                std::ptr::null(),
                // kCFRunLoopAllActivities,
                kCFRunLoopAfterWaiting,
                // kCFRunLoopExit | kCFRunLoopBeforeWaiting,
                YES,                  // repeated
                CFIndex::min_value(), // priority (less is higher)
                // CFIndex::max_value(),
                App::trigger,
                std::ptr::null_mut(),
            );

            CFRunLoopAddObserver(CFRunLoopGetMain(), observer, kCFRunLoopCommonModes);
        };
    }

    pub fn run(&self) {
        unsafe {
            let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];

            if let Some(&mut HandlerState::Running {
                ref mut handler, ..
            }) = get_app_handler(&Some(*self.app_delegate))
            {
                handler.start();
            }

            let () = msg_send![*self.inner, finishLaunching];
            let () = msg_send![*self.inner, run];
            // let _: () = msg_send![pool, release];
            let _: () = msg_send![pool, drain];
        }
    }

    #[allow(unused)]
    pub fn run_by_events(&mut self) {
        loop {
            unsafe {
                let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];

                // Blocks until event available
                let date: ObjcId = msg_send![class!(NSDate), distantPast];
                let nsevent: ObjcId = msg_send![*self.inner,
                        nextEventMatchingMask: NSAnyEventMask
                        untilDate: date
                        inMode:NSDefaultRunLoopMode
                        dequeue:YES];

                let event_type: ObjcId = msg_send![nsevent, type];
                if event_type as u64 == NSApplicationDefined as u64 {
                    let event_subtype: ObjcId = msg_send![nsevent, subtype];
                    if event_subtype as i16
                        == NSEventSubtype::NSApplicationActivatedEventType as i16
                    {
                        let nswindow: ObjcId = msg_send![nsevent, window];
                        let () = msg_send![nswindow, eventLoopAwaken];
                    }
                } else {
                    let () = msg_send![*self.inner, sendEvent: nsevent];
                }

                let date: ObjcId = msg_send![class!(NSDate), distantPast];
                // Get all pending events
                loop {
                    let nsevent: ObjcId = msg_send![*self.inner,
                        nextEventMatchingMask: NSAnyEventMask
                        untilDate: date
                        inMode:NSDefaultRunLoopMode
                        dequeue:YES];
                    let () = msg_send![*self.inner, sendEvent: nsevent];
                    if nsevent == nil {
                        break;
                    }
                }

                let _: () = msg_send![pool, release];
            }
        }
    }

    pub fn clipboard_get() -> Option<String> {
        unsafe {
            let pasteboard: ObjcId = msg_send![class!(NSPasteboard), generalPasteboard];
            let content: ObjcId =
                msg_send![pasteboard, stringForType: NSStringPboardType];
            let string = nsstring_to_string(content);
            if string.is_empty() {
                return None;
            }
            Some(string)
        }
    }

    pub fn clipboard_set(data: &str) {
        let str: ObjcId = str_to_nsstring(data);
        unsafe {
            let pasteboard: ObjcId = msg_send![class!(NSPasteboard), generalPasteboard];
            let () = msg_send![pasteboard, clearContents];
            let arr: ObjcId = msg_send![class!(NSArray), arrayWithObject: str];
            let () = msg_send![pasteboard, writeObjects: arr];
        }
    }

    pub fn confirm_quit() {
        let native_app = NATIVE_APP.get();
        if let Some(app) = native_app {
            unsafe {
                let _: ObjcId = msg_send![*app.inner, terminate: nil];
            }
        }
    }

    pub fn appearance() -> Appearance {
        let native_app = NATIVE_APP.get();
        if let Some(app) = native_app {
            let name = unsafe {
                let appearance: ObjcId = msg_send![*app.inner, effectiveAppearance];
                nsstring_to_string(msg_send![appearance, name])
            };
            log::info!("App Appearance is {name}");
            match name.as_str() {
                "NSAppearanceNameVibrantDark" | "NSAppearanceNameDarkAqua" => {
                    Appearance::Dark
                }
                "NSAppearanceNameVibrantLight" | "NSAppearanceNameAqua" => {
                    Appearance::Light
                }
                "NSAppearanceNameAccessibilityHighContrastVibrantLight"
                | "NSAppearanceNameAccessibilityHighContrastAqua" => {
                    Appearance::LightHighContrast
                }
                "NSAppearanceNameAccessibilityHighContrastVibrantDark"
                | "NSAppearanceNameAccessibilityHighContrastDarkAqua" => {
                    Appearance::DarkHighContrast
                }
                _ => {
                    log::warn!("Unknown NSAppearanceName {name}, assume Light");
                    Appearance::Light
                }
            }
        } else {
            Appearance::Light
        }
    }

    pub fn hide_application() {
        let native_app = NATIVE_APP.get();
        if let Some(app) = native_app {
            let ns_app = *app.inner;
            unsafe {
                let () = msg_send![ns_app, hide: ns_app];
            }
        }
    }
}

pub struct Window {
    pub ns_window: *mut Object,
    pub ns_view: *mut Object,
    pub id: u16,
    pub raw_window_handle: raw_window_handle::RawWindowHandle,
    pub raw_display_handle: raw_window_handle::RawDisplayHandle,
}

impl Window {
    pub async fn new(
        conf: crate::conf::Conf,
    ) -> Result<(Self, (i32, i32, f32)), Box<dyn std::error::Error>> {
        unsafe {
            let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];
            let id = crate::get_handler().lock().next_id();

            crate::set_display(
                id,
                NativeDisplayData {
                    ..NativeDisplayData::new()
                },
            );

            let (app, target) = if let Some(app) = NATIVE_APP.get() {
                (*app.app_delegate, app.target)
            } else {
                (std::ptr::null_mut(), crate::Target::Application)
            };

            let mut display = MacosDisplay {
                has_initialized: false,
                app,
                id,
                has_focus: true,
                view: std::ptr::null_mut(),
                window: std::ptr::null_mut(),
                renderer: std::ptr::null_mut(),
                ime: ImeState::Disabled,
                marked_text: String::from(""),
                fullscreen: false,
                cursor_shown: true,
                current_cursor: CursorIcon::Default,
                cursor_grabbed: false,
                cursors: HashMap::new(),
                // event_handler,
                modifiers: Modifiers::default(),
                screen_width: 0,
                screen_height: 0,
                high_dpi: false,
                dpi_scale: 1.0,
            };

            let window_masks = if conf.hide_toolbar {
                NSWindowStyleMask::NSTitledWindowMask as u64
                // NSWindowStyleMask::NSBorderlessWindowMask as u64
                    | NSWindowStyleMask::NSClosableWindowMask as u64
                    | NSWindowStyleMask::NSMiniaturizableWindowMask as u64
                    | NSWindowStyleMask::NSResizableWindowMask as u64
                    | NSWindowStyleMask::NSFullSizeContentViewWindowMask as u64
            } else {
                NSWindowStyleMask::NSTitledWindowMask as u64
                    | NSWindowStyleMask::NSClosableWindowMask as u64
                    | NSWindowStyleMask::NSMiniaturizableWindowMask as u64
                    | NSWindowStyleMask::NSResizableWindowMask as u64
            };

            let window_frame = NSRect {
                origin: NSPoint { x: 0., y: 0. },
                size: NSSize {
                    width: conf.window_width as f64,
                    height: conf.window_height as f64,
                },
            };

            let window: ObjcId = msg_send![class!(NSWindow), alloc];
            let window = StrongPtr::new(msg_send![
                window,
                initWithContentRect: window_frame
                styleMask: window_masks
                backing: NSBackingStoreType::NSBackingStoreBuffered as u64
                defer: NO
            ]);

            assert!(!window.is_null());

            let title = str_to_nsstring(&conf.window_title);

            let () = msg_send![*window, setReleasedWhenClosed: NO];
            let () = msg_send![*window, setTitle: title];
            let () = msg_send![*window, center];
            let () = msg_send![*window, setAcceptsMouseMovedEvents: YES];

            let view = View::create_metal_view(
                target,
                window_frame,
                conf.sample_count,
                format!("{VIEW_CLASS_NAME}{id}").as_str(),
            );
            {
                let mut d = get_handler().lock();
                let d = d.get_mut(id).unwrap();
                d.view = **view.as_strong_ptr();
            }

            display.window = *window;
            display.view = **view.as_strong_ptr();

            // let window_delegate = StrongPtr::new(msg_send![window_delegate_class, new]);
            let () = msg_send![*window, setDelegate: display.view];
            let () = msg_send![*window, setContentView: display.view];

            let notification_center: &Object =
                msg_send![class!(NSDistributedNotificationCenter), defaultCenter];
            let notification_name =
                str_to_nsstring("AppleInterfaceThemeChangedNotification");
            let () = msg_send![
                notification_center,
                addObserver: **view.as_strong_ptr()
                selector: sel!(appearanceDidChange:)
                name: notification_name
                object: nil
            ];

            {
                let mut d = get_handler().lock();
                let d = d.get_mut(id).unwrap();
                d.window_handle = Some(display.raw_window_handle());
                d.display_handle = Some(display.raw_display_handle());
            }

            // let nstimer: ObjcId = msg_send![
            //     class!(NSTimer),
            //     timerWithTimeInterval: 0.0000001
            //     target: **view.as_strong_ptr()
            //     selector: sel!(timerFired:)
            //     userInfo: nil
            //     repeats: true
            // ];
            // let nsrunloop: ObjcId = msg_send![class!(NSRunLoop), currentRunLoop];
            // let () = msg_send![nsrunloop, addTimer: nstimer forMode: NSDefaultRunLoopMode];

            let raw_window_handle = display.raw_window_handle();
            let raw_display_handle = display.raw_display_handle();

            let boxed_view = Box::into_raw(Box::new(display));

            (*(*boxed_view).view)
                .set_ivar(VIEW_IVAR_NAME, &mut *boxed_view as *mut _ as *mut c_void);

            assert!(!view.is_null());

            // register for drag and drop operations.
            let dragged_arr: ObjcId =
                msg_send![class!(NSArray), arrayWithObject: NSFilenamesPboardType];
            let () = msg_send![
                *window,
                registerForDraggedTypes: dragged_arr
            ];

            if conf.hide_toolbar {
                // let () = msg_send![*window, setMovableByWindowBackground: YES];
                let nswindow_title_hidden = 1;
                let () = msg_send![*window, setTitleVisibility: nswindow_title_hidden];
                let () = msg_send![*window, setTitlebarAppearsTransparent: YES];
            }

            if conf.transparency {
                let () = msg_send![*window, setOpaque: NO];

                let bg_color: ObjcId = msg_send![class!(NSColor), colorWithDeviceRed:0.0 green:0.0 blue:0.0 alpha:0.0];
                let () = msg_send![
                    *window,
                    setBackgroundColor: bg_color
                ];
            }

            if conf.blur {
                let () = msg_send![*window, setHasShadow: NO];
                let window_number: i32 = msg_send![*window, windowNumber];
                CGSSetWindowBackgroundBlurRadius(
                    CGSMainConnectionID(),
                    window_number,
                    80,
                );
            }

            if conf.fullscreen {
                let () = msg_send![*window, toggleFullScreen: nil];
            }

            if conf.hide_toolbar_buttons {
                for titlebar_button in &[
                    NSWindowButton::FullScreen,
                    NSWindowButton::Miniaturize,
                    NSWindowButton::Close,
                    NSWindowButton::Zoom,
                ] {
                    let button: ObjcId =
                        msg_send![*window, standardWindowButton: *titlebar_button];
                    let _: () = msg_send![button, setHidden: YES];
                }
            }

            if !conf.hide_toolbar && conf.tab_identifier.is_some() {
                let () = msg_send![*window,  setTabbingIdentifier: str_to_nsstring(&conf.tab_identifier.unwrap())];
                let _: () = msg_send![*window, setTabbingMode:NSWindowTabbingMode::NSWindowTabbingModePreferred];
            } else {
                let _: () = msg_send![*window, setTabbingMode:NSWindowTabbingMode::NSWindowTabbingModeDisallowed];
            }

            let min_size = NSSize {
                width: 200.,
                height: 200.,
            };
            let _: () = msg_send![*window, setMinSize: min_size];
            let _: () = msg_send![*window, setRestorable: NO];

            let () = msg_send![*window, makeFirstResponder: **view.as_strong_ptr()];
            // let () = msg_send![*window, initialFirstResponder: **view.as_strong_ptr()];
            let () = msg_send![*window, makeKeyAndOrderFront: nil];

            let window_handle = Window {
                id,
                ns_window: *window,
                ns_view: **view.as_strong_ptr(),
                raw_window_handle,
                raw_display_handle,
            };

            let _: () = msg_send![pool, drain];

            let dimensions = initialize_view(window_handle.ns_view.as_ref().unwrap());
            Ok((window_handle, dimensions))
        }
    }
}
