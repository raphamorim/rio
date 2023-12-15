//Originally retired from https://github.com/not-fl3/macroquad licensed under MIT (https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT)
//MacOs implementation is basically a mix between
//sokol_app's objective C code and Makepad's (<https://github.com/makepad/makepad/blob/live/platform/src/platform/apple>)
//platform implementation

use crate::sync::FairMutex;
use raw_window_handle::HasRawDisplayHandle;
use raw_window_handle::HasRawWindowHandle;
use std::sync::Arc;
use objc::rc::{StrongPtr, WeakPtr};
use {
    crate::{
        event::{EventHandler, EventHandlerAction, MouseButton},
        native::{
            apple::{apple_util::*, frameworks::*},
            NativeDisplayData, Request,
        },
        native_display, CursorIcon,
    },
    std::{collections::HashMap, os::raw::c_void, sync::mpsc::Receiver},
};

#[allow(non_upper_case_globals)]
const NSViewLayerContentsPlacementTopLeft: isize = 11;
#[allow(non_upper_case_globals)]
const NSViewLayerContentsRedrawDuringViewResize: isize = 2;

const VIEW_CLASS_NAME: &str = "RioWindowView";
const WINDOW_CLASS_NAME: &str = "RioWindow";

pub struct MacosDisplay {
    window: ObjcId,
    view: ObjcId,
    fullscreen: bool,
    // [NSCursor hide]/unhide calls should be balanced
    // hide/hide/unhide will keep cursor hidden
    // so need to keep internal cursor state to avoid problems from
    // unbalanced show_mouse() calls
    cursor_shown: bool,
    current_cursor: CursorIcon,
    cursor_grabbed: bool,
    cursors: HashMap<CursorIcon, ObjcId>,

    event_handler: Option<Box<dyn EventHandler>>,
    // f: Box<dyn 'static + FnMut(&Window)>,
    f: Option<Box<dyn 'static + FnOnce() -> Box<dyn EventHandler>>>,
    modifiers: Modifiers,
    native_requests: Receiver<Request>,
}

unsafe impl raw_window_handle::HasRawWindowHandle for MacosDisplay {
    fn raw_window_handle(&self) -> raw_window_handle::RawWindowHandle {
        let mut window_handle = raw_window_handle::AppKitWindowHandle::empty();
        window_handle.ns_window = self.window as *mut _;
        window_handle.ns_view = self.view as *mut _;
        raw_window_handle::RawWindowHandle::AppKit(window_handle)
    }
}

unsafe impl raw_window_handle::HasRawDisplayHandle for MacosDisplay {
    fn raw_display_handle(&self) -> raw_window_handle::RawDisplayHandle {
        let handle = raw_window_handle::AppKitDisplayHandle::empty();
        raw_window_handle::RawDisplayHandle::AppKit(handle)
    }
}

impl MacosDisplay {
    fn set_cursor_grab(&mut self, window: *mut Object, grab: bool) {
        if grab == self.cursor_grabbed {
            return;
        }

        self.cursor_grabbed = grab;

        unsafe {
            if grab {
                self.move_mouse_inside_window(window);
                CGAssociateMouseAndMouseCursorPosition(false);
                let () = msg_send![class!(NSCursor), hide];
            } else {
                let () = msg_send![class!(NSCursor), unhide];
                CGAssociateMouseAndMouseCursorPosition(true);
            }
        }
    }
    fn show_mouse(&mut self, show: bool) {
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
    fn set_mouse_cursor(&mut self, cursor: crate::CursorIcon) {
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
    fn set_title(&self, title: &str) {
        unsafe {
            let title = str_to_nsstring(title);
            let _: () = msg_send![&*self.window, setTitle: &*title];
        }
    }
    fn set_subtitle(&self, subtitle: &str) {
        // if !os::is_minimum_version(11) {
        //     return;
        // }

        unsafe {
            let subtitle = str_to_nsstring(subtitle);
            let _: () = msg_send![&*self.window, setSubtitle: &*subtitle];
        }
    }
    fn set_window_size(&mut self, new_width: u32, new_height: u32) {
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
    fn set_fullscreen(&mut self, fullscreen: bool) {
        if self.fullscreen != fullscreen {
            self.fullscreen = fullscreen;
            unsafe {
                let () = msg_send![self.window, toggleFullScreen: nil];
            }
        }
    }
    fn clipboard_get(&mut self) -> Option<String> {
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
    fn clipboard_set(&mut self, data: &str) {
        let str: ObjcId = str_to_nsstring(data);
        unsafe {
            let pasteboard: ObjcId = msg_send![class!(NSPasteboard), generalPasteboard];
            let () = msg_send![pasteboard, clearContents];
            let arr: ObjcId = msg_send![class!(NSArray), arrayWithObject: str];
            let () = msg_send![pasteboard, writeObjects: arr];
        }
    }
    fn confirm_quit(&self, path: &str) -> Option<bool> {
        unsafe {
            let panel: *mut Object = msg_send![class!(NSAlert), new];

            let prompt =
                format!("Do you want to save changes to {} before quitting?", path);
            let title = "Save changes?";
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
                1000 => Some(true),
                1001 => Some(false),
                _ => None,
            }
        }
    }
    pub fn context(&mut self) -> Option<&mut dyn EventHandler> {
        let event_handler = self.event_handler.as_deref_mut()?;

        Some(event_handler)
    }
}

impl MacosDisplay {
    fn transform_mouse_point(&self, point: &NSPoint) -> (f32, f32) {
        let binding = native_display().lock();
        let d = binding.get(0).unwrap();
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
        let mut binding = native_display().lock();
        let d = binding.get_mut(0).unwrap();
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

    fn process_request(&mut self, request: Request) {
        use Request::*;
        match request {
            SetCursorGrab(grab) => self.set_cursor_grab(self.window, grab),
            ShowMouse(show) => self.show_mouse(show),
            SetWindowTitle(title) => self.set_title(&title),
            SetMouseCursor(icon) => self.set_mouse_cursor(icon),
            SetWindowSize {
                new_width,
                new_height,
            } => self.set_window_size(new_width as _, new_height as _),
            SetFullscreen(fullscreen) => self.set_fullscreen(fullscreen),
            // _ => {}
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

// extern "C" fn application_dock_menu(
//     _self: &mut Object,
//     _sel: Sel,
//     _app: *mut Object,
// ) -> *mut Object {
//     // let dock_menu = Menu::new_with_title("");
//     // let new_window_item =
//     //     MenuItem::new_with("New Window", Some(sel!(weztermPerformKeyAssignment:)), "");
//     // new_window_item
//     //     .set_represented_item(RepresentedItem::KeyAssignment(KeyAssignment::SpawnWindow));
//     // dock_menu.add_item(&new_window_item);
//     // dock_menu.autorelease()
// }

extern "C" fn application_open_untitled_file(
    this: &mut Object,
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

extern "C" fn application_did_finish_launching(this: &mut Object, _sel: Sel, _notif: *mut Object) {
    log::debug!("application_did_finish_launching");
    unsafe {
        (*this).set_ivar("launched", YES);
    }
}


extern "C" fn application_open_file(
    this: &mut Object,
    _sel: Sel,
    _app: *mut Object,
    file_name: *mut Object,
) {
    let launched: BOOL = unsafe { *this.get_ivar("launched") };
    if launched == YES {
        let file_name = unsafe { nsstring_to_string(file_name) }.to_string();
        // if let Some(conn) = Connection::get() {
        //     log::debug!("application_open_file {file_name}");
        //     conn.dispatch_app_event(ApplicationEvent::OpenCommandScript(file_name));
        // }
    }
}

pub fn define_app_delegate() -> *const Class {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("NSAppDelegate", superclass).unwrap();
    unsafe {
        // decl.add_method(
        //     sel!(applicationDockMenu:),
        //     application_dock_menu as extern "C" fn(&mut Object, Sel, *mut Object) -> *mut Object,
        // );
        decl.add_method(
            sel!(applicationShouldTerminateAfterLastWindowClosed:),
            yes1 as extern "C" fn(&Object, Sel, ObjcId) -> BOOL,
        );
        decl.add_method(
            sel!(applicationDidFinishLaunching:),
            application_did_finish_launching as extern "C" fn(&mut Object, Sel, *mut Object),
        );
        decl.add_method(
            sel!(application:openFile:),
            application_open_file as extern "C" fn(&mut Object, Sel, *mut Object, *mut Object),
        );
        decl.add_method(
            sel!(applicationOpenUntitledFile:),
            application_open_untitled_file
                as extern "C" fn(&mut Object, Sel, *mut Object) -> BOOL,
        );
    }

    decl.add_ivar::<BOOL>("launched");

    decl.register()
}

#[inline]
fn send_resize_event(payload: &mut MacosDisplay, rescale: bool) {
    if let Some((w, h, scale_factor)) = unsafe { payload.update_dimensions() } {
        if let Some(event_handler) = payload.context() {
            // let s = d.sugarloaf.clone().unwrap();
            // let mut s = s.lock();
            // s.resize(w.try_into().unwrap(), h.try_into().unwrap());
            // if rescale {
            //     s.rescale(scale_factor);
            //     s.resize(w.try_into().unwrap(), h.try_into().unwrap());
            //     s.calculate_bounds();
            // } else {
            //     s.resize(w.try_into().unwrap(), h.try_into().unwrap());
            // }
            event_handler.resize_event(
                w.try_into().unwrap(),
                h.try_into().unwrap(),
                scale_factor,
                rescale,
            );
        }
    }
}

pub fn define_cocoa_window_delegate(window_delegate: &str) -> *const Class {
    extern "C" fn window_should_close(this: &Object, _: Sel, _: ObjcId) -> BOOL {
        let payload = get_window_payload(this);

        if payload.is_none() {
            return NO;
        }

        unsafe {
            let capture_manager =
                msg_send_![class![MTLCaptureManager], sharedCaptureManager];
            msg_send_![capture_manager, stopCapture];
        }

        // only give user-code a chance to intervene when sapp_quit() wasn't already called
        if !native_display().lock().get(0).unwrap().quit_ordered {
            // if window should be closed and event handling is enabled, give user code
            // a chance to intervene via sapp_cancel_quit()
            native_display().lock().get_mut(0).unwrap().quit_requested = true;
            if let Some(event_handler) = payload.unwrap().context() {
                event_handler.quit_requested_event();
            }

            // user code hasn't intervened, quit the app
            if native_display().lock().get(0).unwrap().quit_requested {
                native_display().lock().get_mut(0).unwrap().quit_ordered = true;
            }
        }
        if native_display().lock().get(0).unwrap().quit_ordered {
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
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new(window_delegate, superclass).unwrap();

    // Add callback methods
    unsafe {
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
    }
    // Store internal state as user data
    decl.add_ivar::<*mut c_void>(VIEW_CLASS_NAME);

    decl.register()
}

// methods for both metal or OPENGL view
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
                        .or_insert_with(|| load_mouse_cursor(current_cursor.clone()));
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

    extern "C" fn key_down(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            let mods = get_event_key_modifier(event);
            let repeat: bool = unsafe { msg_send!(event, isARepeat) };
            if let Some(key) = get_event_keycode(event) {
                if let Some(event_handler) = payload.context() {
                    event_handler.key_down_event(key, mods, repeat, get_event_char(event));
                }
            }
        }
    }

    extern "C" fn key_up(this: &Object, _sel: Sel, event: ObjcId) {
        if let Some(payload) = get_window_payload(this) {
            let mods = get_event_key_modifier(event);
            if let Some(key) = get_event_keycode(event) {
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
                } else {
                    if let Some(event_handler) = payload.context() {
                        event_handler.key_up_event(keycode, mods);
                    }
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
    decl.add_method(sel!(keyUp:), key_up as extern "C" fn(&Object, Sel, ObjcId));
}


pub fn define_metal_view_class(view_class_name: &str) -> *const Class {
    let superclass = class!(MTKView);
    let mut decl = ClassDecl::new(view_class_name, superclass).unwrap();
    decl.add_ivar::<*mut c_void>(VIEW_CLASS_NAME);

    decl.add_protocol(
        Protocol::get("NSTextInputClient").expect("failed to get NSTextInputClient protocol"),
    );
    decl.add_protocol(Protocol::get("CALayerDelegate").expect("CALayerDelegate not defined"));

    extern "C" fn display_layer(view: &mut Object, sel: Sel, _layer_id: ObjcId) {
        println!("display_layer");
    }

    extern "C" fn draw_layer_in_context(
        _view: &mut Object,
        _sel: Sel,
        _layer_id: ObjcId,
        _context: ObjcId,
    ) {
    }

    extern "C" fn timer_fired(this: &Object, _sel: Sel, _: ObjcId) {
        unsafe {
            let () = msg_send!(this, setNeedsDisplay: YES);
        }
    }

    extern "C" fn draw_rect(this: &Object, _sel: Sel, _rect: NSRect) {
        if let Some(payload) = get_window_payload(this) {
            if payload.event_handler.is_none() {
                let f = payload.f.take().unwrap();
                payload.event_handler = Some(f());
            }

            while let Ok(request) = payload.native_requests.try_recv() {
                payload.process_request(request);
            }

            if let Some(event_handler) = payload.context() {
                match event_handler.process() {
                    EventHandlerAction::Render => {
                        event_handler.draw();
                    }
                    EventHandlerAction::Update(update_opcode) => {
                        event_handler.update(update_opcode);
                    }
                    EventHandlerAction::Noop => {}
                    EventHandlerAction::Quit => unsafe {
                        let mut handler = native_display().lock();
                        let d = handler.get_mut(0).unwrap();
                        if d.quit_requested || d.quit_ordered {
                            handler.remove(0);
                            let () = msg_send![payload.window, performClose: nil];
                        }
                    },
                    EventHandlerAction::Init => {
                        let mut d = native_display().lock();
                        let d = d.get_mut(0).unwrap();

                        // Initialization should happen only once
                        if !d.has_initialized {
                            {
                                event_handler.init(
                                    1,
                                    d.window_handle.unwrap(),
                                    d.display_handle.unwrap(),
                                    d.dimensions.0,
                                    d.dimensions.1,
                                    d.dimensions.2,
                                );

                                event_handler.resize_event(
                                    d.dimensions.0,
                                    d.dimensions.1,
                                    d.dimensions.2,
                                    true,
                                );
                            }

                            d.has_initialized = true;
                        }
                    }
                }
            }
        }
    }

    unsafe {
        //decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));
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
            sel!(drawLayer:inContext:),
            draw_layer_in_context as extern "C" fn(&mut Object, Sel, ObjcId, ObjcId),
        );

        view_base_decl(&mut decl);
    }

    decl.register()
}

fn get_window_payload(this: &Object) -> Option<&mut MacosDisplay> {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar(VIEW_CLASS_NAME);
        if ptr.is_null() {
            None
        } else {
            Some(&mut *(ptr as *mut MacosDisplay))
        }
    }
}

struct View {
    inner: StrongPtr
}

impl View {
    unsafe fn create_metal_view(_: NSRect, sample_count: i32, class_name: &str, display: &mut MacosDisplay) -> Self {
        let mtl_device_obj = MTLCreateSystemDefaultDevice();
        let view_class = define_metal_view_class(class_name);
        let view: ObjcId = msg_send![view_class, alloc];
        let view: StrongPtr = StrongPtr::new(msg_send![view, init]);

        let boxed_view = Box::into_raw(Box::new(Self {
            inner: StrongPtr::new(*view),
        }));

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
    ns_app: StrongPtr,
    ns_app_delegate: StrongPtr,
}

impl<'a> App {
    pub fn new() -> App {
        crate::set_handler();

        unsafe {
            let app_delegate_class = define_app_delegate();
            let app_delegate_instance = StrongPtr::new(msg_send![app_delegate_class, new]);

            let ns_app = StrongPtr::new(msg_send![class!(NSApplication), sharedApplication]);
            let () = msg_send![*ns_app, setDelegate: *app_delegate_instance];
            let () = msg_send![
                *ns_app,
                setActivationPolicy: NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular
                    as i64
            ];
            let () = msg_send![*ns_app, activateIgnoringOtherApps: YES];

            Self {
                ns_app,
                ns_app_delegate: app_delegate_instance,
            }
        }
    }

    pub fn run(&self) {
        unsafe {
            // let nstimer: ObjcId = msg_send![
            //     class!(NSTimer),
            //     timerWithTimeInterval: 0.001
            //     target: self.ns_view
            //     selector: sel!(timerFired:)
            //     userInfo: nil
            //     repeats: true
            // ];
            // let nsrunloop: ObjcId = msg_send![class!(NSRunLoop), currentRunLoop];
            // let () =
            //     msg_send![nsrunloop, addTimer: nstimer forMode: NSDefaultRunLoopMode];

            let () = msg_send![*self.ns_app, run];
        }
        // let nstimer: ObjcId = msg_send![
        //     class!(NSTimer),
        //     timerWithTimeInterval: 0.001
        //     target: self.displays.get(&0).unwrap().view
        //     selector: sel!(timerFired:)
        //     userInfo: nil
        //     repeats: true
        // ];
        // let nsrunloop: ObjcId = msg_send![class!(NSRunLoop), currentRunLoop];
        // let () = msg_send![nsrunloop, addTimer: nstimer forMode: NSDefaultRunLoopMode];

        // let () = msg_send![self.ns_app, finishLaunching];
        // // let () = msg_send![self.ns_app, run];
    }
}

pub struct Window {
    id: usize,
    pub ns_window: *mut Object,
    pub ns_view: *mut Object,
}

unsafe impl Send for Window {}
unsafe impl Sync for Window {}

impl Window {
    pub async fn new_window<F>(
        _class_name: &str,
        _name: &str,
        conf: crate::conf::Conf,
        f: F) -> Result<Self, Box<dyn std::error::Error>>
    where
        // F: 'static + FnMut(&Window),
        F: 'static + FnOnce() -> Box<dyn EventHandler>,
    {
        unsafe {
            let (tx, rx) = std::sync::mpsc::channel();
            let clipboard = Box::new(MacosClipboard);

            crate::set_display(
                0,
                NativeDisplayData {
                    ..NativeDisplayData::new(
                        conf.window_width,
                        conf.window_height,
                        tx,
                        clipboard,
                    )
                },
            );

            let mut display = MacosDisplay {
                view: std::ptr::null_mut(),
                window: std::ptr::null_mut(),
                fullscreen: false,
                cursor_shown: true,
                current_cursor: CursorIcon::Default,
                cursor_grabbed: false,
                cursors: HashMap::new(),
                f: Some(Box::new(f)),
                event_handler: None,
                native_requests: rx,
                modifiers: Modifiers::default(),
            };

            // let autoreleasePool: *mut Object = msg_send![class!(NSAutoreleasePool), new];

            // let window_masks = if conf.hide_toolbar {
            //     NSWindowStyleMask::NSBorderlessWindowMask as u64
            //         | NSWindowStyleMask::NSMiniaturizableWindowMask as u64
            //         | NSWindowStyleMask::NSResizableWindowMask as u64
            // } else {
            let window_masks = if conf.hide_toolbar {
                NSWindowStyleMask::NSTitledWindowMask as u64
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
                styleMask: window_masks as u64
                backing: NSBackingStoreType::NSBackingStoreBuffered as u64
                defer: NO
            ]);

            assert!(!window.is_null());

            let window_delegate_class = define_cocoa_window_delegate("RenderViewClass");
            let window_delegate = StrongPtr::new(msg_send![window_delegate_class, new]);
            let () = msg_send![*window, setDelegate: *window_delegate];

            let title = str_to_nsstring(&conf.window_title);

            let () = msg_send![*window, setReleasedWhenClosed: NO];
            let () = msg_send![*window, setTitle: title];
            let () = msg_send![*window, center];
            let () = msg_send![*window, setAcceptsMouseMovedEvents: YES];

            let view = View::create_metal_view(
                window_frame,
                conf.sample_count,
                "RenderWindowDelegate",
                &mut display
            );
            {
                let mut d = native_display().lock();
                let d = d.get_mut(0).unwrap();
                d.view = **view.as_strong_ptr();
            }

            display.window = *window;
            display.view = **view.as_strong_ptr();

            let () = msg_send![*window, setContentView: **view.as_strong_ptr()];

            let dimensions = display.update_dimensions().unwrap_or((
                conf.window_width,
                conf.window_height,
                2.0,
            ));
            {
                let mut d = native_display().lock();
                let d = d.get_mut(0).unwrap();
                d.window_handle = Some(display.raw_window_handle());
                d.display_handle = Some(display.raw_display_handle());
                d.dimensions = dimensions;
            }

            let boxed_view = Box::into_raw(Box::new(display));

            (*(*boxed_view).view)
                .set_ivar(VIEW_CLASS_NAME, &mut *boxed_view as *mut _ as *mut c_void);

            (**window_delegate)
                .set_ivar(VIEW_CLASS_NAME, &mut *boxed_view as *mut _ as *mut c_void);

            // let nstimer: ObjcId = msg_send![
            //     class!(NSTimer),
            //     timerWithTimeInterval: 0.001
            //     target: *view
            //     selector: sel!(timerFired:)
            //     userInfo: nil
            //     repeats: true
            // ];
            // let nsrunloop: ObjcId = msg_send![class!(NSRunLoop), currentRunLoop];
            // let () = msg_send![nsrunloop, addTimer: nstimer forMode: NSDefaultRunLoopMode];
            assert!(!view.is_null());

            if conf.hide_toolbar {
                // let () = msg_send![window, setMovableByWindowBackground: YES];
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

            if !conf.hide_toolbar {
                let () =
                    msg_send![*window,  setTabbingIdentifier: str_to_nsstring("tab-1")];
                let _: () = msg_send![*window, setTabbingMode:NSWindowTabbingMode::NSWindowTabbingModePreferred];
            } else {
                let _: () = msg_send![*window, setTabbingMode:NSWindowTabbingMode::NSWindowTabbingModeDisallowed];
            }

            let _: () = msg_send![*window, setRestorable: NO];

            let () = msg_send![*window, makeFirstResponder: **view.as_strong_ptr()];
            let () = msg_send![*window, makeKeyAndOrderFront: nil];

            let window_handle = Window {
                id: 0,
                ns_window: *window,
                ns_view: **view.as_strong_ptr(),
            };

            Ok(window_handle)
        }
    }
    
}