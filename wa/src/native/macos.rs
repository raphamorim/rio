//! Originally retired from https://github.com/not-fl3/macroquad licensed under MIT (https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT)
//! MacOs implementation is basically a mix between
//! sokol_app's objective C code and Makepad's (<https://github.com/makepad/makepad/blob/live/platform/src/platform/apple>)
//! platform implementation
//!
use {
    crate::{
        conf::AppleGfxApi,
        event::{EventHandler, MouseButton},
        graphics::create_sugarloaf_instance,
        native::{
            apple::{apple_util::*, frameworks::*},
            NativeDisplayData, Request,
        },
        native_display, CursorIcon,
    },
    std::{collections::HashMap, os::raw::c_void, sync::mpsc::Receiver},
};

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
    gfx_api: crate::conf::AppleGfxApi,

    event_handler: Option<Box<dyn EventHandler>>,
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

    pub fn context(&mut self) -> Option<&mut dyn EventHandler> {
        let event_handler = self.event_handler.as_deref_mut()?;

        Some(event_handler)
    }
}

impl MacosDisplay {
    fn transform_mouse_point(&self, point: &NSPoint) -> (f32, f32) {
        let d = native_display().lock().unwrap();
        let new_x = point.x as f32 * d.dpi_scale;
        let new_y = d.screen_height as f32 - (point.y as f32 * d.dpi_scale) - 1.;

        (new_x, new_y)
    }

    fn move_mouse_inside_window(&self, window: *mut Object) {
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
        let mut d = native_display().lock().unwrap();
        let mut current_dpi_scale = 1.0;
        if d.high_dpi {
            let screen: ObjcId = msg_send![self.window, screen];
            let dpi_scale: f64 = msg_send![screen, backingScaleFactor];
            current_dpi_scale = dpi_scale as f32;
        }

        d.dpi_scale = current_dpi_scale;

        let bounds: NSRect = msg_send![self.view, bounds];
        let screen_width = (bounds.size.width as f32 * d.dpi_scale) as i32;
        let screen_height = (bounds.size.height as f32 * d.dpi_scale) as i32;

        let dim_changed =
            screen_width != d.screen_width || screen_height != d.screen_height;

        d.screen_width = screen_width;
        d.screen_height = screen_height;

        if dim_changed {
            Some((screen_width, screen_height, current_dpi_scale))
        } else {
            None
        }
    }

    fn process_request(&mut self, request: Request) {
        use Request::*;
        match request {
            SetCursorGrab(grab) => self.set_cursor_grab(self.window, grab),
            ShowMouse(show) => self.show_mouse(show),
            SetMouseCursor(icon) => self.set_mouse_cursor(icon),
            SetWindowSize {
                new_width,
                new_height,
            } => self.set_window_size(new_width as _, new_height as _),
            SetFullscreen(fullscreen) => self.set_fullscreen(fullscreen),
            _ => {}
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
pub fn define_app_delegate() -> *const Class {
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("NSAppDelegate", superclass).unwrap();
    unsafe {
        decl.add_method(
            sel!(applicationShouldTerminateAfterLastWindowClosed:),
            yes1 as extern "C" fn(&Object, Sel, ObjcId) -> BOOL,
        );
    }

    return decl.register();
}

pub fn define_cocoa_window_delegate() -> *const Class {
    extern "C" fn window_should_close(this: &Object, _: Sel, _: ObjcId) -> BOOL {
        let payload = get_window_payload(this);

        unsafe {
            let capture_manager =
                msg_send_![class![MTLCaptureManager], sharedCaptureManager];
            msg_send_![capture_manager, stopCapture];
        }

        // only give user-code a chance to intervene when sapp_quit() wasn't already called
        if !native_display().lock().unwrap().quit_ordered {
            // if window should be closed and event handling is enabled, give user code
            // a chance to intervene via sapp_cancel_quit()
            native_display().lock().unwrap().quit_requested = true;
            if let Some(event_handler) = payload.context() {
                event_handler.quit_requested_event();
            }

            // user code hasn't intervened, quit the app
            if native_display().lock().unwrap().quit_requested {
                native_display().lock().unwrap().quit_ordered = true;
            }
        }
        if native_display().lock().unwrap().quit_ordered {
            return YES;
        } else {
            return NO;
        }
    }

    extern "C" fn window_did_resize(this: &Object, _: Sel, _: ObjcId) {
        let payload = get_window_payload(this);
        if let Some((w, h, _scale_factor)) = unsafe { payload.update_dimensions() } {
            if let Some(event_handler) = payload.context() {
                event_handler.resize_event(w as _, h as _);
            }
        }
    }

    extern "C" fn window_did_change_screen(this: &Object, _: Sel, _: ObjcId) {
        let payload = get_window_payload(this);
        if let Some((w, h, _scale_factor)) = unsafe { payload.update_dimensions() } {
            if let Some(event_handler) = payload.context() {
                event_handler.resize_event(w as _, h as _);
            }
        }
    }
    extern "C" fn window_did_enter_fullscreen(this: &Object, _: Sel, _: ObjcId) {
        let payload = get_window_payload(this);
        payload.fullscreen = true;
    }
    extern "C" fn window_did_exit_fullscreen(this: &Object, _: Sel, _: ObjcId) {
        let payload = get_window_payload(this);
        payload.fullscreen = false;
    }
    let superclass = class!(NSObject);
    let mut decl = ClassDecl::new("RenderWindowDelegate", superclass).unwrap();

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
    decl.add_ivar::<*mut c_void>("display_ptr");

    return decl.register();
}

// methods for both metal or OPENGL view
unsafe fn view_base_decl(decl: &mut ClassDecl) {
    extern "C" fn mouse_moved(this: &Object, _sel: Sel, event: ObjcId) {
        let payload = get_window_payload(this);

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

    fn fire_mouse_event(this: &Object, event: ObjcId, down: bool, btn: MouseButton) {
        let payload = get_window_payload(this);

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
        let payload = get_window_payload(this);
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
    extern "C" fn reset_cursor_rects(this: &Object, _sel: Sel) {
        let payload = get_window_payload(this);

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

    extern "C" fn key_down(this: &Object, _sel: Sel, event: ObjcId) {
        let payload = get_window_payload(this);
        let mods = get_event_key_modifier(event);
        let repeat: bool = unsafe { msg_send!(event, isARepeat) };
        if let Some(key) = get_event_keycode(event) {
            if let Some(event_handler) = payload.context() {
                event_handler.key_down_event(key, mods, repeat);
            }
        }

        if let Some(character) = get_event_char(event) {
            if let Some(event_handler) = payload.context() {
                event_handler.char_event(character, mods, repeat);
            }
        }
    }

    extern "C" fn key_up(this: &Object, _sel: Sel, event: ObjcId) {
        let payload = get_window_payload(this);
        let mods = get_event_key_modifier(event);
        if let Some(key) = get_event_keycode(event) {
            if let Some(event_handler) = payload.context() {
                event_handler.key_up_event(key, mods);
            }
        }
    }

    extern "C" fn flags_changed(this: &Object, _sel: Sel, event: ObjcId) {
        fn produce_event(
            payload: &mut MacosDisplay,
            keycode: crate::KeyCode,
            mods: crate::KeyMods,
            old_pressed: bool,
            new_pressed: bool,
        ) {
            if new_pressed ^ old_pressed {
                if new_pressed {
                    if let Some(event_handler) = payload.context() {
                        event_handler.key_down_event(keycode, mods, false);
                    }
                } else {
                    if let Some(event_handler) = payload.context() {
                        event_handler.key_up_event(keycode, mods);
                    }
                }
            }
        }

        let payload = get_window_payload(this);
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

pub fn define_opengl_view_class() -> *const Class {
    //extern "C" fn dealloc(this: &Object, _sel: Sel) {}

    extern "C" fn reshape(this: &Object, _sel: Sel) {
        let payload = get_window_payload(this);

        unsafe {
            let superclass = superclass(this);
            let () = msg_send![super(this, superclass), reshape];

            if let Some((w, h, scale_factor)) = payload.update_dimensions() {
                if let Some(event_handler) = payload.context() {
                    event_handler.resize_event(w as _, h as _);
                }
            }
        }
    }

    extern "C" fn draw_rect(this: &Object, _sel: Sel, _rect: NSRect) {
        let payload = get_window_payload(this);

        while let Ok(request) = payload.native_requests.try_recv() {
            payload.process_request(request);
        }

        if let Some(event_handler) = payload.context() {
            event_handler.update();
            event_handler.draw();
        }

        unsafe {
            let ctx: ObjcId = msg_send![this, openGLContext];
            assert!(!ctx.is_null());
            let () = msg_send![ctx, flushBuffer];

            let d = native_display().lock().unwrap();
            if d.quit_requested || d.quit_ordered {
                drop(d);
                let () = msg_send![payload.window, performClose: nil];
            }
        }
    }

    extern "C" fn prepare_open_gl(this: &Object, _sel: Sel) {
        let payload = get_window_payload(this);
        unsafe {
            let superclass = superclass(this);
            let () = msg_send![super(this, superclass), prepareOpenGL];
            let mut swap_interval = 1;
            let ctx: ObjcId = msg_send![this, openGLContext];
            let () = msg_send![ctx,
                               setValues:&mut swap_interval
                               forParameter:NSOpenGLContextParameterSwapInterval];
            let () = msg_send![ctx, makeCurrentContext];
        }

        let f = payload.f.take().unwrap();
        payload.event_handler = Some(f());
    }

    extern "C" fn timer_fired(this: &Object, _sel: Sel, _: ObjcId) {
        unsafe {
            let () = msg_send!(this, setNeedsDisplay: YES);
        }
    }
    let superclass = class!(NSOpenGLView);
    let mut decl: ClassDecl = ClassDecl::new("RenderViewClass", superclass).unwrap();
    unsafe {
        //decl.add_method(sel!(dealloc), dealloc as extern "C" fn(&Object, Sel));
        decl.add_method(
            sel!(timerFired:),
            timer_fired as extern "C" fn(&Object, Sel, ObjcId),
        );

        decl.add_method(
            sel!(prepareOpenGL),
            prepare_open_gl as extern "C" fn(&Object, Sel),
        );
        decl.add_method(sel!(reshape), reshape as extern "C" fn(&Object, Sel));
        decl.add_method(
            sel!(drawRect:),
            draw_rect as extern "C" fn(&Object, Sel, NSRect),
        );

        view_base_decl(&mut decl);
    }

    decl.add_ivar::<*mut c_void>("display_ptr");

    return decl.register();
}

pub fn define_metal_view_class() -> *const Class {
    let superclass = class!(MTKView);
    let mut decl = ClassDecl::new("RenderViewClass", superclass).unwrap();
    decl.add_ivar::<*mut c_void>("display_ptr");

    extern "C" fn timer_fired(this: &Object, _sel: Sel, _: ObjcId) {
        unsafe {
            let () = msg_send!(this, setNeedsDisplay: YES);
        }
    }

    extern "C" fn draw_rect(this: &Object, _sel: Sel, _rect: NSRect) {
        let payload = get_window_payload(this);

        if payload.event_handler.is_none() {
            let f = payload.f.take().unwrap();
            payload.event_handler = Some(f());
        }

        while let Ok(request) = payload.native_requests.try_recv() {
            payload.process_request(request);
        }

        if let Some(event_handler) = payload.context() {
            event_handler.update();
            event_handler.draw();
        }

        unsafe {
            let d = native_display().lock().unwrap();
            if d.quit_requested || d.quit_ordered {
                drop(d);
                let () = msg_send![payload.window, performClose: nil];
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

        view_base_decl(&mut decl);
    }

    return decl.register();
}

fn get_window_payload(this: &Object) -> &mut MacosDisplay {
    unsafe {
        let ptr: *mut c_void = *this.get_ivar("display_ptr");
        &mut *(ptr as *mut MacosDisplay)
    }
}

unsafe fn create_metal_view(_: NSRect, sample_count: i32, _: bool) -> ObjcId {
    let mtl_device_obj = MTLCreateSystemDefaultDevice();
    let view_class = define_metal_view_class();
    let view: ObjcId = msg_send![view_class, alloc];
    let view: ObjcId = msg_send![view, init];

    let () = msg_send![view, setDevice: mtl_device_obj];
    let () = msg_send![view, setColorPixelFormat: MTLPixelFormat::BGRA8Unorm];
    let () = msg_send![
        view,
        setDepthStencilPixelFormat: MTLPixelFormat::Depth32Float_Stencil8
    ];
    let () = msg_send![view, setSampleCount: sample_count];

    view
}

unsafe fn create_opengl_view(
    window_frame: NSRect,
    sample_count: i32,
    high_dpi: bool,
) -> ObjcId {
    use NSOpenGLPixelFormatAttribute::*;

    let mut attrs: Vec<u32> = vec![];

    attrs.push(NSOpenGLPFAAccelerated as _);
    attrs.push(NSOpenGLPFADoubleBuffer as _);
    attrs.push(NSOpenGLPFAOpenGLProfile as _);
    attrs.push(NSOpenGLPFAOpenGLProfiles::NSOpenGLProfileVersion3_2Core as _);
    attrs.push(NSOpenGLPFAColorSize as _);
    attrs.push(24);
    attrs.push(NSOpenGLPFAAlphaSize as _);
    attrs.push(8);
    attrs.push(NSOpenGLPFADepthSize as _);
    attrs.push(24);
    attrs.push(NSOpenGLPFAStencilSize as _);
    attrs.push(8);
    if sample_count > 1 {
        attrs.push(NSOpenGLPFAMultisample as _);
        attrs.push(NSOpenGLPFASampleBuffers as _);
        attrs.push(1 as _);
        attrs.push(NSOpenGLPFASamples as _);
        attrs.push(sample_count as _);
    } else {
        attrs.push(NSOpenGLPFASampleBuffers as _);
        attrs.push(0);
    }
    attrs.push(0);

    let glpixelformat_obj: ObjcId = msg_send![class!(NSOpenGLPixelFormat), alloc];
    let glpixelformat_obj: ObjcId =
        msg_send![glpixelformat_obj, initWithAttributes: attrs.as_ptr()];
    assert!(!glpixelformat_obj.is_null());

    let view_class = define_opengl_view_class();
    let view: ObjcId = msg_send![view_class, alloc];
    let view: ObjcId = msg_send![
        view,
        initWithFrame: window_frame
        pixelFormat: glpixelformat_obj
    ];

    if high_dpi {
        let () = msg_send![view, setWantsBestResolutionOpenGLSurface: YES];
    } else {
        let () = msg_send![view, setWantsBestResolutionOpenGLSurface: NO];
    }

    view
}

struct MacosClipboard;
impl crate::native::Clipboard for MacosClipboard {
    fn get(&mut self) -> Option<String> {
        None
    }
    fn set(&mut self, _data: &str) {}
}

#[inline]
pub unsafe fn run<F>(conf: crate::conf::Conf, f: F)
where
    F: 'static + FnOnce() -> Box<dyn EventHandler>,
{
    let (tx, rx) = std::sync::mpsc::channel();
    let clipboard = Box::new(MacosClipboard);
    crate::set_display(NativeDisplayData {
        // high_dpi: conf.high_dpi,
        high_dpi: true,
        gfx_api: conf.platform.apple_gfx_api,
        ..NativeDisplayData::new(conf.window_width, conf.window_height, tx, clipboard)
    });

    let mut display = MacosDisplay {
        view: std::ptr::null_mut(),
        window: std::ptr::null_mut(),
        fullscreen: false,
        cursor_shown: true,
        current_cursor: CursorIcon::Default,
        cursor_grabbed: false,
        cursors: HashMap::new(),
        gfx_api: conf.platform.apple_gfx_api,
        f: Some(Box::new(f)),
        event_handler: None,
        native_requests: rx,
        modifiers: Modifiers::default(),
    };

    let app_delegate_class = define_app_delegate();
    let app_delegate_instance: ObjcId = msg_send![app_delegate_class, new];

    let ns_app: ObjcId = msg_send![class!(NSApplication), sharedApplication];
    let () = msg_send![ns_app, setDelegate: app_delegate_instance];
    let () = msg_send![
        ns_app,
        setActivationPolicy: NSApplicationActivationPolicy::NSApplicationActivationPolicyRegular
            as i64
    ];
    let () = msg_send![ns_app, activateIgnoringOtherApps: YES];

    let window_masks = NSWindowStyleMask::NSTitledWindowMask as u64
        | NSWindowStyleMask::NSClosableWindowMask as u64
        | NSWindowStyleMask::NSMiniaturizableWindowMask as u64
        | NSWindowStyleMask::NSResizableWindowMask as u64;
    //| NSWindowStyleMask::NSFullSizeContentViewWindowMask as u64;

    let window_frame = NSRect {
        origin: NSPoint { x: 0., y: 0. },
        size: NSSize {
            width: conf.window_width as f64,
            height: conf.window_height as f64,
        },
    };

    let window: ObjcId = msg_send![class!(NSWindow), alloc];
    let window: ObjcId = msg_send![
        window,
        initWithContentRect: window_frame
        styleMask: window_masks as u64
        backing: NSBackingStoreType::NSBackingStoreBuffered as u64
        defer: NO
    ];

    assert!(!window.is_null());

    let window_delegate_class = define_cocoa_window_delegate();
    let window_delegate: ObjcId = msg_send![window_delegate_class, new];
    let () = msg_send![window, setDelegate: window_delegate];

    (*window_delegate).set_ivar("display_ptr", &mut display as *mut _ as *mut c_void);

    let title = str_to_nsstring(&conf.window_title);
    //let () = msg_send![window, setReleasedWhenClosed: NO];
    let () = msg_send![window, setTitle: title];
    let () = msg_send![window, center];
    let () = msg_send![window, setAcceptsMouseMovedEvents: YES];

    let view = match conf.platform.apple_gfx_api {
        AppleGfxApi::OpenGl => {
            create_opengl_view(window_frame, conf.sample_count, conf.high_dpi)
        }
        AppleGfxApi::Metal => {
            create_metal_view(window_frame, conf.sample_count, conf.high_dpi)
        }
        AppleGfxApi::WebGPU => {
            create_metal_view(window_frame, conf.sample_count, conf.high_dpi)
        }
    };
    {
        let mut d = native_display().lock().unwrap();
        d.view = view;
    }
    (*view).set_ivar("display_ptr", &mut display as *mut _ as *mut c_void);

    display.window = window;
    display.view = view;

    let () = msg_send![window, setContentView: view];

    let dimensions = display
        .update_dimensions()
        .unwrap_or_else(|| (conf.window_width, conf.window_height, 1.0));

    let sugarloaf_instance = create_sugarloaf_instance(
        display,
        dimensions.0 as f32,
        dimensions.1 as f32,
        dimensions.2,
    );
    {
        let mut d = native_display().lock().unwrap();
        d.sugarloaf = Box::new(sugarloaf_instance);
    }

    let nstimer: ObjcId = msg_send![
        class!(NSTimer),
        timerWithTimeInterval: 0.001
        target: view
        selector: sel!(timerFired:)
        userInfo: nil
        repeats: true
    ];
    let nsrunloop: ObjcId = msg_send![class!(NSRunLoop), currentRunLoop];
    let () = msg_send![nsrunloop, addTimer: nstimer forMode: NSDefaultRunLoopMode];
    assert!(!view.is_null());

    let () = msg_send![window, makeFirstResponder: view];

    if conf.fullscreen {
        let () = msg_send![window, toggleFullScreen: nil];
    }

    let () = msg_send![window, makeKeyAndOrderFront: nil];

    let bg_color: ObjcId =
        msg_send![class!(NSColor), colorWithDeviceRed:0.0 green:0.0 blue:0.0 alpha:1.];
    let () = msg_send![
        window,
        setBackgroundColor: bg_color
    ];

    let ns_app: ObjcId = msg_send![class!(NSApplication), sharedApplication];

    let () = msg_send![ns_app, run];

    // run should never return
    // but just in case
    unreachable!();
}
