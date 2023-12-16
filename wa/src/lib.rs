// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT (https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT) and slightly modified

#![allow(clippy::all)]
#![cfg(target_os = "macos")]

pub mod conf;
mod event;
pub mod native;
pub mod sync;

pub use event::*;

mod resources;

use once_cell::sync::OnceCell;
use sync::FairMutex;

static NATIVE_DISPLAY: OnceCell<FairMutex<native::Handler>> = OnceCell::new();

fn set_handler() {
    let _ = NATIVE_DISPLAY.set(FairMutex::new(native::Handler::new()));
}

fn get_handler() -> &'static FairMutex<native::Handler> {
    NATIVE_DISPLAY
        .get()
        .expect("Backend has not initialized NATIVE_DISPLAY yet.") //|| Mutex::new(Default::default()))
}

fn set_display(id: u16, display: native::NativeDisplayData) {
    let handler: &FairMutex<native::Handler> = get_handler();
    let mut d = handler.lock();
    d.insert(id, display);
    // let _ = NATIVE_DISPLAY.set(FairMutex::new(display));
}

fn native_display() -> &'static FairMutex<native::Handler> {
    NATIVE_DISPLAY
        .get()
        .expect("Backend has not initialized NATIVE_DISPLAY yet.") //|| Mutex::new(Default::default()))
}

/// Window and associated to window rendering context related functions.
/// in macroquad <= 0.3, it was ctx.screen_size(). Now it is window::screen_size()
pub mod window {
    use super::*;
    /// The current framebuffer size in pixels
    /// NOTE: [High DPI Rendering](../conf/index.html#high-dpi-rendering)
    pub fn screen_size(id: u16) -> (f32, f32) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            (d.screen_width as f32, d.screen_height as f32)
        } else {
            (800., 600.)
        }
    }

    /// The dpi scaling factor (window pixels to framebuffer pixels)
    /// NOTE: [High DPI Rendering](../conf/index.html#high-dpi-rendering)
    pub fn dpi_scale(id: u16) -> f32 {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            d.dpi_scale
        } else {
            1.0
        }
    }

    /// True when high_dpi was requested and actually running in a high-dpi scenario
    /// NOTE: [High DPI Rendering](../conf/index.html#high-dpi-rendering)
    pub fn high_dpi(id: u16) -> bool {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            d.high_dpi
        } else {
            true
        }
    }

    /// This function simply quits the application without
    /// giving the user a chance to intervene. Usually this might
    /// be called when the user clicks the 'Ok' button in a 'Really Quit?'
    /// dialog box
    /// Window might not be actually closed right away (exit(0) might not
    /// happen in the order_quit implmentation) and execution might continue for some time after
    /// But the window is going to be inevitably closed at some point.
    pub fn order_quit(id: u16) {
        let mut d = native_display().lock();
        if let Some(d) = d.get_mut(id) {
            d.quit_ordered = true;
        }
    }

    /// Shortcut for `order_quit`. Will add a legacy attribute at some point.
    pub fn quit(id: u16) {
        order_quit(id)
    }

    /// Calling request_quit() will trigger "quit_requested_event" event , giving
    /// the user code a chance to intervene and cancel the pending quit process
    /// (for instance to show a 'Really Quit?' dialog box).
    /// If the event handler callback does nothing, the application will be quit as usual.
    /// To prevent this, call the function "cancel_quit()"" from inside the event handler.
    pub fn request_quit(id: u16) {
        let mut d = native_display().lock();
        if let Some(d) = d.get_mut(id) {
            d.quit_requested = true;
        }
    }

    /// Cancels a pending quit request, either initiated
    /// by the user clicking the window close button, or programmatically
    /// by calling "request_quit()". The only place where calling this
    /// function makes sense is from inside the event handler callback when
    /// the "quit_requested_event" event has been received
    pub fn cancel_quit(id: u16) {
        let mut d = native_display().lock();
        if let Some(d) = d.get_mut(id) {
            d.quit_requested = false;
        }
    }
    /// Capture mouse cursor to the current window
    /// On WASM this will automatically hide cursor
    /// On desktop this will bound cursor to windows border
    /// NOTICE: on desktop cursor will not be automatically released after window lost focus
    ///         so set_cursor_grab(false) on window's focus lost is recommended.
    /// TODO: implement window focus events
    pub fn set_cursor_grab(id: u16, grab: bool) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            let _ = d.native_requests.send(native::Request::SetCursorGrab(grab));
        }
    }

    /// Show or hide the mouse cursor
    pub fn show_mouse(id: u16, shown: bool) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            let _ = d.native_requests.send(native::Request::ShowMouse(shown));
        }
    }

    /// Show or hide the mouse cursor
    pub fn set_window_title(id: u16, title: String, subtitle: String) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            let _ = d
                .native_requests
                .send(native::Request::SetWindowTitle(title, subtitle));
        }
    }

    /// Set the mouse cursor icon.
    pub fn set_mouse_cursor(id: u16, cursor_icon: CursorIcon) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            let _ = d
                .native_requests
                .send(native::Request::SetMouseCursor(cursor_icon));
        }
    }

    /// Set the application's window size.
    pub fn set_window_size(id: u16, new_width: u32, new_height: u32) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            let _ = d.native_requests.send(native::Request::SetWindowSize {
                new_width,
                new_height,
            });
        }
    }

    pub fn set_fullscreen(id: u16, fullscreen: bool) {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            let _ = d
                .native_requests
                .send(native::Request::SetFullscreen(fullscreen));
        }
    }

    /// Get current OS clipboard value
    pub fn clipboard_get(id: u16) -> Option<String> {
        let mut d = native_display().lock();
        if let Some(d) = d.get_mut(id) {
            d.clipboard.get()
        } else {
            Some(String::from(""))
        }
    }
    /// Save value to OS clipboard
    pub fn clipboard_set(id: u16, data: &str) {
        let mut d = native_display().lock();
        if let Some(d) = d.get_mut(id) {
            d.clipboard.set(data)
        }
    }
    pub fn dropped_file_count(id: u16) -> usize {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            d.dropped_files.bytes.len()
        } else {
            0
        }
    }
    pub fn dropped_file_bytes(id: u16, index: usize) -> Option<Vec<u8>> {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            d.dropped_files.bytes.get(index).cloned()
        } else {
            None
        }
    }
    pub fn dropped_file_path(id: u16, index: usize) -> Option<std::path::PathBuf> {
        let d = native_display().lock();
        if let Some(d) = d.get(id) {
            d.dropped_files.paths.get(index).cloned()
        } else {
            None
        }
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

// pub fn run()
// {
//     #[cfg(target_os = "macos")]
//     unsafe {
//         native::macos::run();
//     }
// }

// pub fn create_app()  {
//     #[cfg(target_os = "macos")]
//     unsafe {
//         native::macos::create_app();
//     }
// }

// pub fn create_window<F>(conf: conf::Conf, f: F)
// where
//     F: 'static + FnOnce() -> Box<dyn EventHandler>,
// {
//     #[cfg(target_os = "macos")]
//     unsafe {
//         native::macos::create_window(conf, f);
//     }
// }
