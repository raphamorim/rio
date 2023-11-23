// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT (https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT) and slightly modified

#![allow(clippy::all)]

pub mod conf;
mod event;
pub mod graphics;
mod native;

pub use event::*;

mod default_icon;

use std::sync::{Mutex, OnceLock};

static NATIVE_DISPLAY: OnceLock<Mutex<native::NativeDisplayData>> = OnceLock::new();

fn set_display(display: native::NativeDisplayData) {
    let _ = NATIVE_DISPLAY.set(Mutex::new(display));
}
fn native_display() -> &'static Mutex<native::NativeDisplayData> {
    NATIVE_DISPLAY
        .get()
        .expect("Backend has not initialized NATIVE_DISPLAY yet.") //|| Mutex::new(Default::default()))
}

/// Window and associated to window rendering context related functions.
/// in macroquad <= 0.3, it was ctx.screen_size(). Now it is window::screen_size()
pub mod window {
    use super::*;

    /// The same as
    /// ```ignore
    /// if metal {
    ///    Box::new(MetalContext::new())
    /// } else {
    ///   Box::new(GlContext::new())
    /// };
    /// ```
    /// but under #[cfg] gate to avoid MetalContext on non-apple platforms
    // pub fn new_rendering_backend() -> Box<dyn RenderingBackend> {
    //     #[cfg(target_vendor = "apple")]
    //     {
    //         if window::apple_gfx_api() == conf::AppleGfxApi::Metal {
    //             Box::new(MetalContext::new())
    //         } else {
    //             Box::new(GlContext::new())
    //         }
    //     }
    //     #[cfg(not(target_vendor = "apple"))]
    //     Box::new(GlContext::new())
    // }

    /// The current framebuffer size in pixels
    /// NOTE: [High DPI Rendering](../conf/index.html#high-dpi-rendering)
    pub fn screen_size() -> (f32, f32) {
        let d = native_display().lock().unwrap();
        (d.screen_width as f32, d.screen_height as f32)
    }

    /// The dpi scaling factor (window pixels to framebuffer pixels)
    /// NOTE: [High DPI Rendering](../conf/index.html#high-dpi-rendering)
    pub fn dpi_scale() -> f32 {
        let d = native_display().lock().unwrap();
        d.dpi_scale
    }

    /// True when high_dpi was requested and actually running in a high-dpi scenario
    /// NOTE: [High DPI Rendering](../conf/index.html#high-dpi-rendering)
    pub fn high_dpi() -> bool {
        let d = native_display().lock().unwrap();
        d.high_dpi
    }

    /// This function simply quits the application without
    /// giving the user a chance to intervene. Usually this might
    /// be called when the user clicks the 'Ok' button in a 'Really Quit?'
    /// dialog box
    /// Window might not be actually closed right away (exit(0) might not
    /// happen in the order_quit implmentation) and execution might continue for some time after
    /// But the window is going to be inevitably closed at some point.
    pub fn order_quit() {
        let mut d = native_display().lock().unwrap();
        d.quit_ordered = true;
    }

    /// Shortcut for `order_quit`. Will add a legacy attribute at some point.
    pub fn quit() {
        order_quit()
    }

    /// Calling request_quit() will trigger "quit_requested_event" event , giving
    /// the user code a chance to intervene and cancel the pending quit process
    /// (for instance to show a 'Really Quit?' dialog box).
    /// If the event handler callback does nothing, the application will be quit as usual.
    /// To prevent this, call the function "cancel_quit()"" from inside the event handler.
    pub fn request_quit() {
        let mut d = native_display().lock().unwrap();
        d.quit_requested = true;
    }

    /// Cancels a pending quit request, either initiated
    /// by the user clicking the window close button, or programmatically
    /// by calling "request_quit()". The only place where calling this
    /// function makes sense is from inside the event handler callback when
    /// the "quit_requested_event" event has been received
    pub fn cancel_quit() {
        let mut d = native_display().lock().unwrap();
        d.quit_requested = false;
    }
    /// Capture mouse cursor to the current window
    /// On WASM this will automatically hide cursor
    /// On desktop this will bound cursor to windows border
    /// NOTICE: on desktop cursor will not be automatically released after window lost focus
    ///         so set_cursor_grab(false) on window's focus lost is recommended.
    /// TODO: implement window focus events
    pub fn set_cursor_grab(grab: bool) {
        let d = native_display().lock().unwrap();
        let _ = d.native_requests.send(native::Request::SetCursorGrab(grab));
    }

    /// Show or hide the mouse cursor
    pub fn show_mouse(shown: bool) {
        let d = native_display().lock().unwrap();
        let _ = d.native_requests.send(native::Request::ShowMouse(shown));
    }

    /// Set the mouse cursor icon.
    pub fn set_mouse_cursor(cursor_icon: CursorIcon) {
        let d = native_display().lock().unwrap();
        let _ = d
            .native_requests
            .send(native::Request::SetMouseCursor(cursor_icon));
    }

    /// Set the application's window size.
    pub fn set_window_size(new_width: u32, new_height: u32) {
        let d = native_display().lock().unwrap();
        let _ = d.native_requests.send(native::Request::SetWindowSize {
            new_width,
            new_height,
        });
    }

    pub fn set_fullscreen(fullscreen: bool) {
        let d = native_display().lock().unwrap();
        let _ = d
            .native_requests
            .send(native::Request::SetFullscreen(fullscreen));
    }

    /// Get current OS clipboard value
    pub fn clipboard_get() -> Option<String> {
        let mut d = native_display().lock().unwrap();
        d.clipboard.get()
    }

    /// Save value to OS clipboard
    pub fn clipboard_set(data: &str) {
        let mut d = native_display().lock().unwrap();
        d.clipboard.set(data)
    }
    pub fn dropped_file_count() -> usize {
        let d = native_display().lock().unwrap();
        d.dropped_files.bytes.len()
    }
    pub fn dropped_file_bytes(index: usize) -> Option<Vec<u8>> {
        let d = native_display().lock().unwrap();
        d.dropped_files.bytes.get(index).cloned()
    }
    pub fn dropped_file_path(index: usize) -> Option<std::path::PathBuf> {
        let d = native_display().lock().unwrap();
        d.dropped_files.paths.get(index).cloned()
    }

    /// Show/hide onscreen keyboard.
    /// Only works on Android right now.
    pub fn show_keyboard(show: bool) {
        let d = native_display().lock().unwrap();
        let _ = d.native_requests.send(native::Request::ShowKeyboard(show));
    }

    #[cfg(target_vendor = "apple")]
    pub fn apple_view() -> crate::native::apple::frameworks::ObjcId {
        let d = native_display().lock().unwrap();
        d.view
    }
    #[cfg(target_ios = "ios")]
    pub fn apple_view_ctrl() -> crate::native::apple::frameworks::ObjcId {
        let d = native_display().lock().unwrap();
        d.view_ctrl
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq)]
pub enum CursorIcon {
    Default,
    Help,
    Pointer,
    Wait,
    Crosshair,
    Text,
    Move,
    NotAllowed,
    EWResize,
    NSResize,
    NESWResize,
    NWSEResize,
}

pub fn start<F>(conf: conf::Conf, f: F)
where
    F: 'static + FnOnce() -> Box<dyn EventHandler>,
{
    #[cfg(target_os = "macos")]
    unsafe {
        native::macos::run(conf, f);
    }
}
