// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT
// https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT
// The code has suffered several changes like support to multiple windows, extension of windows
// properties, menu support, IME support, and etc.

use crate::native::apple::menu::{KeyAssignment, Menu, MenuItem, RepresentedItem};
use crate::native::macos::NSEventMask::NSAnyEventMask;
use crate::native::macos::NSEventType::NSApplicationDefined;
use crate::FairMutex;
use crate::{AppHandler, Appearance};
use objc::rc::StrongPtr;
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, RawDisplayHandle, RawWindowHandle,
};
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

// #[allow(non_upper_case_globals)]
// const NSViewLayerContentsPlacementTopLeft: isize = 11;
#[allow(non_upper_case_globals)]
const NSViewLayerContentsRedrawDuringViewResize: isize = 2;

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

pub static NATIVE_APP: OnceLock<FairMutex<App>> = OnceLock::new();
pub static NATIVE_APP_EVENTS: OnceLock<FairMutex<Vec<RepresentedItem>>> = OnceLock::new();

#[cfg(target_pointer_width = "32")]
pub type NSInteger = libc::c_int;
#[cfg(target_pointer_width = "32")]
pub type NSUInteger = libc::c_uint;

#[cfg(target_pointer_width = "64")]
pub type NSInteger = libc::c_long;
#[cfg(target_pointer_width = "64")]
pub type NSUInteger = libc::c_ulong;

#[derive(Debug)]
#[repr(C)]
struct NSRangePointer(*mut NSRange);

unsafe impl objc::Encode for NSRangePointer {
    fn encode() -> objc::Encoding {
        unsafe { objc::Encoding::from_str(&format!("^{}", NSRange::encode().as_str())) }
    }
}

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

pub struct MacosDisplay {
    window: ObjcId,
    view: ObjcId,
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

    event_handler: Option<Box<dyn EventHandler>>,
    // f: Box<dyn 'static + FnMut(&Window)>,
    f: Option<Box<dyn 'static + FnOnce() -> Box<dyn EventHandler>>>,
    modifiers: Modifiers,
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

// impl HasWindowHandle for SugarloafWindow {
//     fn window_handle(&self) -> std::result::Result<WindowHandle, HandleError> {
//         let raw = self.raw_window_handle();
//         Ok(unsafe { WindowHandle::borrow_raw(raw) })
//     }
// }

// impl HasDisplayHandle for SugarloafWindow {
//     fn display_handle(&self) -> Result<DisplayHandle, HandleError> {
//         let raw = self.raw_display_handle();
//         Ok(unsafe { DisplayHandle::borrow_raw(raw)})
//     }
// }

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
    #[inline]
    pub fn context(&mut self) -> Option<&mut dyn EventHandler> {
        let event_handler = self.event_handler.as_deref_mut()?;

        Some(event_handler)
    }
}

// fn get_dimensions(window: *mut Object, view: *mut Object) -> (i32, i32, f32) {
//     let screen: ObjcId = msg_send![window, screen];
//     let dpi_scale: f64 = msg_send![screen, backingScaleFactor];
//     let dpi_scale = dpi_scale as f32;

//     let bounds: NSRect = msg_send![view, bounds];
//     let screen_width = (bounds.size.width as f32 * dpi_scale) as i32;
//     let screen_height = (bounds.size.height as f32 * dpi_scale) as i32;
//     (screen_width, screen_height, dpi_scale)
// }

impl MacosDisplay {
    fn transform_mouse_point(&self, point: &NSPoint) -> (f32, f32) {
        let binding = get_handler().lock();
        let d = binding.get(self.id).unwrap();
        let new_x = point.x as f32 * d.dpi_scale;
        let new_y = d.screen_height as f32 - (point.y as f32 * d.dpi_scale) - 1.;

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
        let mut binding = get_handler().lock();
        let d = binding.get_mut(self.id).unwrap();
        let screen: ObjcId = msg_send![self.window, screen];
        let dpi_scale: f64 = msg_send![screen, backingScaleFactor];
        d.dpi_scale = dpi_scale as f32;

        let bounds: NSRect = msg_send![self.view, bounds];
        let screen_width = (bounds.size.width as f32 * d.dpi_scale) as i32;
        let screen_height = (bounds.size.height as f32 * d.dpi_scale) as i32;

        let dim_changed =
            screen_width != d.screen_width || screen_height != d.screen_height;

        d.screen_width = screen_width;
        d.screen_height = screen_height;

        if dim_changed {
            Some((screen_width, screen_height, d.dpi_scale))
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
                let sender = NATIVE_APP_EVENTS.get();
                if let Some(channel) = sender {
                    let mut events = channel.lock();
                    events.push(action);
                }
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

#[allow(dead_code)]
extern "C" fn application_open_untitled_file(
    this: &Object,
    _sel: Sel,
    _app: *mut Object,
) -> BOOL {
    let launched: BOOL = unsafe { *this.get_ivar("launched") };
    log::debug!("application_open_untitled_file launched={launched}");
    // if let Some(conn) = Connection::get() {
    // if launched == YES {
    // conn.dispatch_app_event(ApplicationEvent::PerformKeyAssignment(
    //     KeyAssignment::SpawnWindow,
    // ));
    // }
    // return YES;
    // }
    NO
}

extern "C" fn application_did_finish_launching(
    this: &mut Object,
    _sel: Sel,
    _notif: *mut Object,
) {
    log::debug!("application_did_finish_launching");
    unsafe {
        (*this).set_ivar("launched", YES);
    }
}

extern "C" fn application_open_urls(
    this: &Object,
    _sel: Sel,
    _sender: ObjcId,
    urls: ObjcId,
) {
    if let Some(payload) = get_window_payload(this) {
        unsafe {
            let count: u64 = msg_send![urls, count];
            if count > 0 {
                let mut urls_to_send = vec![];
                for index in 0..count {
                    let item = msg_send![urls, objectAtIndex: index];
                    let path = nsstring_to_string(item);
                    urls_to_send.push(path);
                }

                if let Some(event_handler) = payload.context() {
                    event_handler.open_urls_event(urls_to_send);
                }
            }
        }
    }
}

#[allow(dead_code)]
extern "C" fn application_open_file(
    this: &Object,
    _sel: Sel,
    _app: *mut Object,
    file_name: *mut Object,
) {
    let launched: BOOL = unsafe { *this.get_ivar("launched") };
    if launched == YES {
        let file_name = nsstring_to_string(file_name).to_string();
        if let Some(payload) = get_window_payload(this) {
            if let Some(event_handler) = payload.context() {
                event_handler.open_file_event(file_name);
            }
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
        // decl.add_method(
        //     sel!(application:openFile:),
        //     application_open_file
        //         as extern "C" fn(&Object, Sel, *mut Object, *mut Object),
        // );
        decl.add_method(
            sel!(rioPerformKeyAssignment:),
            rio_perform_key_assignment as extern "C" fn(&Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(application:openURLs:),
            application_open_urls as extern "C" fn(&Object, Sel, ObjcId, ObjcId),
        );
        // decl.add_method(
        //     sel!(applicationOpenUntitledFile:),
        //     application_open_untitled_file
        //         as extern "C" fn(&Object, Sel, *mut Object) -> BOOL,
        // );
    }

    decl.add_ivar::<BOOL>("launched");

    decl.register()
}

#[inline]
fn send_resize_event(payload: &mut MacosDisplay, rescale: bool) {
    if let Some((w, h, scale_factor)) = unsafe { payload.update_dimensions() } {
        if let Some(event_handler) = payload.context() {
            event_handler.resize_event(w, h, scale_factor, rescale);
        }
    }
}

#[inline]
unsafe fn view_base_decl(decl: &mut ClassDecl) {
    extern "C" fn mouse_moved(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            unsafe {
                if payload.cursor_grabbed {
                    let dx: f64 = msg_send!(event, deltaX);
                    let dy: f64 = msg_send!(event, deltaY);
                    if let Some(event_handler) = payload.context() {
                        event_handler.raw_mouse_motion(dx as f32, dy as f32);
                    }
                } else {
                    let point: NSPoint = msg_send!(event, locationInWindow);
                    let point = payload.transform_mouse_point(&point);
                    if let Some(event_handler) = payload.context() {
                        event_handler.mouse_motion_event(point.0, point.1);
                    }
                }
            }
        }
    }

    fn fire_mouse_event(this: &Object, event: ObjcId, down: bool, btn: MouseButton) {
        if let Some(payload) = get_window_payload(this) {
            unsafe {
                let point: NSPoint = msg_send!(event, locationInWindow);
                let point = payload.transform_mouse_point(&point);
                if let Some(event_handler) = payload.context() {
                    if down {
                        event_handler.mouse_button_down_event(btn, point.0, point.1);
                    } else {
                        event_handler.mouse_button_up_event(btn, point.0, point.1);
                    }
                }
            }
        }
    }

    extern "C" fn do_command_by_selector(this: &Object, _sel: Sel, _a_selector: Sel) {
        // println!("do_command_by_selector");
        if let Some(payload) = get_window_payload(this) {
            if payload.ime == ImeState::Commited {
                return;
            }

            if !payload.marked_text.is_empty() && payload.ime == ImeState::Preedit {
                payload.ime = ImeState::Ground;
            }
        }
    }

    extern "C" fn has_marked_text(this: &Object, _sel: Sel) -> BOOL {
        // println!("has_marked_text");
        if let Some(payload) = get_window_payload(this) {
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
        // println!("marked_range");
        if let Some(payload) = get_window_payload(this) {
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
        // println!("selected_range");
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
        // println!("insertText:replacementRange:");

        let string = nsstring_to_string(astring);
        let is_control = string.chars().next().map_or(false, |c| c.is_control());

        if !is_control {
            if let Some(payload) = get_window_payload(this) {
                // Commit only if we have marked text.
                if !payload.marked_text.is_empty() && payload.ime != ImeState::Disabled {
                    if let Some(event_handler) = payload.context() {
                        event_handler
                            .ime_event(crate::ImeState::Preedit(String::new(), None));
                        event_handler.ime_event(crate::ImeState::Commit(string));
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
        // println!("setMarkedText:selectedRange:replacementRange:");
        let s = nsstring_to_string(astring);

        if let Some(payload) = get_window_payload(this) {
            let preedit_string: String = s.to_string();
            payload.marked_text = preedit_string.clone();

            if payload.ime == ImeState::Disabled {
                if let Some(event_handler) = payload.context() {
                    event_handler.ime_event(crate::ImeState::Enabled);
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

            if let Some(event_handler) = payload.context() {
                event_handler
                    .ime_event(crate::ImeState::Preedit(preedit_string, cursor_range));
            }
        }
    }

    extern "C" fn unmark_text(this: &Object, _sel: Sel) {
        // println!("unmarkText");
        if let Some(payload) = get_window_payload(this) {
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
        // println!("character_index_for_point");
        NSNOT_FOUND as _
        // 0
    }

    extern "C" fn first_rect_for_character_range(
        this: &Object,
        _sel: Sel,
        _range: NSRange,
        _actual: *mut c_void,
    ) -> NSRect {
        // println!("first_rect_for_character_range");

        // Returns a rect in screen coordinates; this is used to place
        // the input method editor
        let window: ObjcId = unsafe { msg_send![this, window] };
        let frame: NSRect = unsafe { msg_send![window, frame] };

        let content: NSRect =
            unsafe { msg_send![window, contentRectForFrameRect: frame] };
        // let backing_frame: NSRect =
        //     unsafe { msg_send![this, convertRectToBacking: frame] };

        if let Some(payload) = get_window_payload(this) {
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
        // println!("valid_attributes_for_marked_text");

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
        // println!("attributed_substring_for_proposed_range");
        nil
    }

    extern "C" fn mouse_down(this: &Object, _sel: Sel, event: ObjcId) {
        fire_mouse_event(this, event, true, MouseButton::Left);
    }
    extern "C" fn mouse_up(this: &Object, _sel: Sel, event: ObjcId) {
        fire_mouse_event(this, event, false, MouseButton::Left);
    }
    extern "C" fn right_mouse_down(this: &Object, _sel: Sel, event: ObjcId) {
        fire_mouse_event(this, event, true, MouseButton::Right);
    }
    extern "C" fn right_mouse_up(this: &Object, _sel: Sel, event: ObjcId) {
        fire_mouse_event(this, event, false, MouseButton::Right);
    }
    extern "C" fn other_mouse_down(this: &Object, _sel: Sel, event: ObjcId) {
        fire_mouse_event(this, event, true, MouseButton::Middle);
    }
    extern "C" fn other_mouse_up(this: &Object, _sel: Sel, event: ObjcId) {
        fire_mouse_event(this, event, false, MouseButton::Middle);
    }
    extern "C" fn scroll_wheel(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            unsafe {
                let mut dx: f64 = msg_send![event, scrollingDeltaX];
                let mut dy: f64 = msg_send![event, scrollingDeltaY];

                if !msg_send![event, hasPreciseScrollingDeltas] {
                    dx *= 10.0;
                    dy *= 10.0;
                }
                if let Some(event_handler) = payload.context() {
                    event_handler.mouse_wheel_event(dx as f32, dy as f32);
                }
            }
        }
    }
    extern "C" fn reset_cursor_rects(this: &Object, _sel: Sel) {
        if let Some(payload) = get_window_payload(this) {
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
        if let Some(payload) = get_window_payload(this) {
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

                    if let Some(event_handler) = payload.context() {
                        event_handler.files_dragged_event(
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
        if let Some(payload) = get_window_payload(this) {
            if let Some(event_handler) = payload.context() {
                event_handler.files_dragged_event(vec![], crate::DragState::Exited);
            }
        }
    }

    extern "C" fn perform_drag_operation(this: &Object, _: Sel, sender: ObjcId) -> BOOL {
        if let Some(payload) = get_window_payload(this) {
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

                    if let Some(event_handler) = payload.context() {
                        event_handler.files_dropped_event(dropped_files);
                    }
                }
            }
        }
        YES
    }

    extern "C" fn window_should_close(this: &Object, _: Sel, _: ObjcId) -> BOOL {
        let payload = get_window_payload(this);

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
            if let Some(event_handler) = payload.context() {
                event_handler.quit_requested_event();
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
        if let Some(payload) = get_window_payload(this) {
            send_resize_event(payload, false);
        }
    }

    extern "C" fn window_did_change_screen(this: &Object, _: Sel, _: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            send_resize_event(payload, true);
        }
    }
    extern "C" fn window_did_enter_fullscreen(this: &Object, _: Sel, _: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            payload.fullscreen = true;
        }
    }
    extern "C" fn window_did_exit_fullscreen(this: &Object, _: Sel, _: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            payload.fullscreen = false;
        }
    }

    extern "C" fn key_down(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            let repeat: bool = unsafe { msg_send!(event, isARepeat) };
            let unmod = unsafe { msg_send!(event, charactersIgnoringModifiers) };
            let unmod = nsstring_to_string(unmod);
            let mods = get_event_key_modifier(event);
            let chars = get_event_char(event);

            log::info!(
                "KEY_DOWN (chars={:?} unmod={:?} modifiers={:?}",
                chars,
                unmod,
                mods
            );

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
                    if let Some(event_handler) = payload.context() {
                        event_handler.key_down_event(
                            key,
                            mods,
                            repeat,
                            get_event_char(event),
                        );
                    }
                }
            }
        }
    }

    extern "C" fn appearance_did_change(this: &Object, _sel: Sel, _app: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            if let Some(event_handler) = payload.context() {
                event_handler.appearance_change_event(App::appearance());
            }
        }
    }

    extern "C" fn key_up(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            let mods = get_event_key_modifier(event);
            if let Some(key) = get_event_keycode(event) {
                log::info!("KEY_UP (key={:?} modifiers={:?}", key, mods);
                if let Some(event_handler) = payload.context() {
                    event_handler.key_up_event(key, mods);
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
                    if let Some(event_handler) = payload.context() {
                        event_handler.key_down_event(keycode, mods, false, None);
                    }
                } else if let Some(event_handler) = payload.context() {
                    event_handler.key_up_event(keycode, mods);
                }
            }
        }

        if let Some(payload) = get_window_payload(this) {
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
        sel!(keyDown:),
        key_down as extern "C" fn(&Object, Sel, ObjcId),
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
    if let Some(payload) = get_window_payload(this) {
        if !payload.has_initialized {
            let id = payload.id;

            unsafe { payload.update_dimensions() };

            if payload.event_handler.is_none() {
                let f = payload.f.take().unwrap();
                payload.event_handler = Some(f());
            }

            let d = get_handler().lock();
            let d = d.get(id).unwrap();
            if let Some(event_handler) = payload.context() {
                event_handler.init(
                    id,
                    d.window_handle.unwrap(),
                    d.display_handle.unwrap(),
                    d.screen_width,
                    d.screen_height,
                    d.dpi_scale,
                );

                event_handler.resize_event(
                    d.screen_width,
                    d.screen_height,
                    d.dpi_scale,
                    true,
                );
            }

            payload.has_initialized = true;
            return;
        }

        if let Some(event_handler) = payload.context() {
            event_handler.process();
        }
    }
}

pub fn define_metal_view_class(view_class_name: &str) -> *const Class {
    let superclass = class!(MTKView);
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

    extern "C" fn wants_update_layer(_view: &mut Object, _sel: Sel) -> BOOL {
        YES
    }

    extern "C" fn draw_layer_in_context(
        _view: &mut Object,
        _sel: Sel,
        _layer_id: ObjcId,
        _context: ObjcId,
    ) {
    }

    extern "C" fn update_layer(_this: &mut Object, _sel: Sel) {
        log::trace!("update_layer called");
    }

    extern "C" fn timer_fired(this: &Object, _sel: Sel, _: ObjcId) {
        unsafe {
            let () = msg_send!(this, setNeedsDisplay: YES);
        }
    }

    extern "C" fn dealloc(this: &Object, _sel: Sel) {
        unsafe {
            let superclass = class!(MTKView);
            let () = msg_send![super(this, superclass), dealloc];
        }
    }

    // extern "C" fn make_backing_layer(this: &mut Object, _: Sel) -> ObjcId {
    //     log::trace!("make_backing_layer");
    //     let class = class!(CAMetalLayer);
    //     unsafe {
    //         let layer: ObjcId = msg_send![class, new];
    //         let () = msg_send![layer, setDelegate: view];
    //         let () = msg_send![layer, setContentsScale: 1.0];
    //         let () = msg_send![layer, setOpaque: NO];
    //         layer
    //     }
    // }

    unsafe {
        decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));
        decl.add_method(
            sel!(timerFired:),
            timer_fired as extern "C" fn(&Object, Sel, ObjcId),
        );
        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, NSRect),
        );
        decl.add_method(
            sel!(displayLayer:),
            display_layer as extern "C" fn(&mut Object, Sel, ObjcId),
        );
        decl.add_method(
            sel!(updateLayer),
            update_layer as extern "C" fn(&mut Object, Sel),
        );
        decl.add_method(
            sel!(wantsUpdateLayer),
            wants_update_layer as extern "C" fn(&mut Object, Sel) -> BOOL,
        );
        decl.add_method(
            sel!(drawLayer:inContext:),
            draw_layer_in_context as extern "C" fn(&mut Object, Sel, ObjcId, ObjcId),
        );

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

pub fn get_window_payload(this: &Object) -> Option<&mut MacosDisplay> {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(VIEW_IVAR_NAME);
        if ptr.is_null() {
            None
        } else {
            Some(&mut *(ptr as *mut MacosDisplay))
        }
    }
}

struct View {
    inner: StrongPtr,
}

impl View {
    unsafe fn create_metal_view(_: NSRect, sample_count: i32, class_name: &str) -> Self {
        let mtl_device_obj = MTLCreateSystemDefaultDevice();
        let view_class = define_metal_view_class(class_name);
        let view: ObjcId = msg_send![view_class, alloc];
        let view: StrongPtr = StrongPtr::new(msg_send![view, init]);

        // let boxed_view = Box::into_raw(Box::new(Self {
        //     inner: StrongPtr::new(*view),
        // }));

        let () = msg_send![*view, setDevice: mtl_device_obj];
        let () = msg_send![*view, setColorPixelFormat: MTLPixelFormat::BGRA8Unorm];
        let () = msg_send![
            *view,
            setDepthStencilPixelFormat: MTLPixelFormat::Depth32Float_Stencil8
        ];
        let () = msg_send![*view, setSampleCount: sample_count];
        // let () = msg_send![*view, setWantsLayer: YES];
        // let () = msg_send![
        //     *view,
        //     setLayerContentsPlacement: NSViewLayerContentsPlacementTopLeft
        // ];

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

struct MacosClipboard;
impl crate::native::Clipboard for MacosClipboard {
    fn get(&mut self) -> Option<String> {
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
    fn set(&mut self, data: &str) {
        let str: ObjcId = str_to_nsstring(data);
        unsafe {
            let pasteboard: ObjcId = msg_send![class!(NSPasteboard), generalPasteboard];
            let () = msg_send![pasteboard, clearContents];
            let arr: ObjcId = msg_send![class!(NSArray), arrayWithObject: str];
            let () = msg_send![pasteboard, writeObjects: arr];
        }
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
    pub ns_app: StrongPtr,
    handler: Box<dyn AppHandler>,
}

unsafe impl Send for App {}
unsafe impl Sync for App {}

impl App {
    pub fn start<F>(f: F) -> StrongPtr
    where
        F: 'static + FnOnce() -> Box<dyn AppHandler>,
    {
        crate::set_handler();

        unsafe {
            let app_delegate_class = define_app_delegate();
            let app_delegate_instance =
                StrongPtr::new(msg_send![app_delegate_class, new]);

            let ns_app =
                StrongPtr::new(msg_send![class!(NSApplication), sharedApplication]);
            let () = msg_send![*ns_app, setDelegate: *app_delegate_instance];
            let () = msg_send![
                *ns_app,
                setActivationPolicy: NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular
                    as i64
            ];
            let () = msg_send![*ns_app, activateIgnoringOtherApps: YES];

            let _ = NATIVE_APP.set(FairMutex::new(App {
                ns_app: ns_app.clone(),
                handler: f(),
            }));

            let _ = NATIVE_APP_EVENTS.set(FairMutex::new(vec![]));

            ns_app
        }
    }

    #[inline]
    pub fn create_window(&mut self) {
        self.handler.create_window();
    }

    extern "C" fn trigger(
        _observer: *mut __CFRunLoopObserver,
        _: CFRunLoopActivity,
        _: *mut std::ffi::c_void,
    ) {
        let sender = NATIVE_APP_EVENTS.get();
        if let Some(events) = sender {
            let events = events.lock();
            if events.is_empty() {
                return;
            }

            let mut events = events;
            if let Some(RepresentedItem::KeyAssignment(KeyAssignment::SpawnWindow)) =
                events.pop()
            {
                let native_app = NATIVE_APP.get();
                if let Some(app) = native_app {
                    let mut app = app.lock();
                    app.create_window();
                }
            }
            // let mut events = channel.lock();
            // events.push(action);
        }

        // Run loop
        // unsafe {
        // CFRunLoopWakeUp(CFRunLoopGetMain());
        // }
    }

    pub fn run() {
        let native_app = NATIVE_APP.get();
        if let Some(app) = native_app {
            let mut app = app.lock();
            app.handler.init();
            let ns_app = *app.ns_app;
            drop(app);

            let observer = unsafe {
                CFRunLoopObserverCreate(
                    std::ptr::null(),
                    kCFRunLoopAllActivities,
                    YES,
                    0,
                    App::trigger,
                    std::ptr::null_mut(),
                )
            };
            unsafe {
                CFRunLoopAddObserver(CFRunLoopGetMain(), observer, kCFRunLoopCommonModes);
            }

            unsafe {
                let () = msg_send![ns_app, finishLaunching];
                let () = msg_send![ns_app, run];
            }
        }
    }

    #[allow(unused)]
    pub fn run_by_events(&mut self) {
        loop {
            unsafe {
                let pool: ObjcId = msg_send![class!(NSAutoreleasePool), new];

                // Blocks until event available
                let date: ObjcId = msg_send![class!(NSDate), distantPast];
                let nsevent: ObjcId = msg_send![*self.ns_app,
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
                    let () = msg_send![*self.ns_app, sendEvent: nsevent];
                }

                let date: ObjcId = msg_send![class!(NSDate), distantPast];
                // Get all pending events
                loop {
                    let nsevent: ObjcId = msg_send![*self.ns_app,
                        nextEventMatchingMask: NSAnyEventMask
                        untilDate: date
                        inMode:NSDefaultRunLoopMode
                        dequeue:YES];
                    let () = msg_send![*self.ns_app, sendEvent: nsevent];
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
            let app = app.lock();
            unsafe {
                let _: ObjcId = msg_send![*app.ns_app, terminate: nil];
            }
        }
    }

    pub fn appearance() -> Appearance {
        let native_app = NATIVE_APP.get();
        if let Some(app) = native_app {
            let app = app.lock();
            let name = unsafe {
                let appearance: ObjcId = msg_send![*app.ns_app, effectiveAppearance];
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
            let app = app.lock();
            let ns_app = *app.ns_app;
            unsafe {
                let () = msg_send![ns_app, hide: ns_app];
            }
        }
    }
}

pub struct Window {
    pub ns_window: *mut Object,
    pub ns_view: *mut Object,
}

impl Window {
    pub async fn new_window<F>(
        conf: crate::conf::Conf,
        f: F,
    ) -> Result<Self, Box<dyn std::error::Error>>
    where
        F: 'static + FnOnce() -> Box<dyn EventHandler>,
    {
        unsafe {
            let clipboard = Box::new(MacosClipboard);

            let id = crate::get_handler().lock().next_id();

            crate::set_display(
                id,
                NativeDisplayData {
                    ..NativeDisplayData::new(
                        conf.window_width,
                        conf.window_height,
                        clipboard,
                    )
                },
            );

            let mut display = MacosDisplay {
                has_initialized: false,
                id,
                view: std::ptr::null_mut(),
                window: std::ptr::null_mut(),
                ime: ImeState::Disabled,
                marked_text: String::from(""),
                fullscreen: false,
                cursor_shown: true,
                current_cursor: CursorIcon::Default,
                cursor_grabbed: false,
                cursors: HashMap::new(),
                f: Some(Box::new(f)),
                event_handler: None,
                modifiers: Modifiers::default(),
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

            // let window_delegate_class = define_cocoa_window_delegate(
            //     format!("RenderViewClassWithId{id}").as_str(),
            // );

            let title = str_to_nsstring(&conf.window_title);

            let () = msg_send![*window, setReleasedWhenClosed: NO];
            let () = msg_send![*window, setTitle: title];
            let () = msg_send![*window, center];
            let () = msg_send![*window, setAcceptsMouseMovedEvents: YES];

            let view = View::create_metal_view(
                window_frame,
                conf.sample_count,
                format!("{VIEW_CLASS_NAME}{id}").as_str(),
            );
            {
                let mut d = get_handler().lock();
                let d = d.get_mut(id).unwrap();
                d.view = **view.as_strong_ptr();
            }

            let () = msg_send![**view.as_strong_ptr(), setWantsLayer: YES];
            let () = msg_send![
                **view.as_strong_ptr(),
                setLayerContentsRedrawPolicy: NSViewLayerContentsRedrawDuringViewResize
            ];

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

            // (**window_delegate)
            //     .set_ivar(VIEW_CLASS_NAME, &mut *boxed_view as *mut _ as *mut c_void);

            // let nstimer: ObjcId = msg_send![
            //     class!(NSTimer),
            //     timerWithTimeInterval: 0.001
            //     target: **view.as_strong_ptr()
            //     selector: sel!(timerFired:)
            //     userInfo: nil
            //     repeats: true
            // ];
            // let nsrunloop: ObjcId = msg_send![class!(NSRunLoop), currentRunLoop];
            // let () = msg_send![nsrunloop, addTimer: nstimer forMode: NSDefaultRunLoopMode];

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
                let () = msg_send![*window, setTitleVisibility: YES];
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
            let () = msg_send![*window, makeKeyAndOrderFront: nil];

            let window_handle = Window {
                ns_window: *window,
                ns_view: **view.as_strong_ptr(),
            };

            Ok(window_handle)
        }
    }
}
