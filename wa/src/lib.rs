// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT
// https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT
// The code has suffered several changes like support to multiple windows, extension of windows
// properties, menu support and etc.

#![cfg(target_os = "macos")]

pub mod app;
pub mod conf;
mod event;
pub mod event_loop;
pub mod native;
mod resources;
pub mod sync;
pub use event::*;

use once_cell::sync::OnceCell;
use sync::FairMutex;

macro_rules! unwrap_or_return {
    ( $e:expr ) => {
        match $e {
            Some(x) => x,
            None => return,
        }
    };
}

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
    handler.lock().insert(id, display);
}

pub mod window {
    use super::*;
    pub fn screen_size(id: u16) -> (f32, f32) {
        let d = get_handler().lock();
        if let Some(d) = d.get(id) {
            (d.screen_width as f32, d.screen_height as f32)
        } else {
            (800., 600.)
        }
    }
    pub fn dpi_scale(id: u16) -> f32 {
        let d = get_handler().lock();
        if let Some(d) = d.get(id) {
            d.dpi_scale
        } else {
            1.0
        }
    }
    pub fn high_dpi(id: u16) -> bool {
        let d = get_handler().lock();
        if let Some(d) = d.get(id) {
            d.high_dpi
        } else {
            true
        }
    }
    pub fn order_quit(id: u16) {
        let mut d = get_handler().lock();
        if let Some(d) = d.get_mut(id) {
            d.quit_ordered = true;
        }
    }
    pub fn quit(id: u16) {
        order_quit(id)
    }
    pub fn request_quit() {
        App::confirm_quit()
    }
    pub fn cancel_quit(id: u16) {
        let mut d = get_handler().lock();
        if let Some(d) = d.get_mut(id) {
            d.quit_requested = false;
        }
    }
    pub fn set_cursor_grab(id: u16, grab: bool) {
        let d = get_handler().lock();
        if let Some(display) = d.get(id) {
            let view = display.view;
            unsafe {
                if let Some(display) = native::macos::get_display_payload(&*view) {
                    display.set_cursor_grab(grab);
                }
            }
        }
    }
    /// Show or hide the mouse cursor
    pub fn show_mouse(id: u16, shown: bool) {
        let d = get_handler().lock();
        let view = unwrap_or_return!(d.get(id)).view;
        // drop view as soon we have it, if let Some() keeps locked until block drop
        drop(d);

        unsafe {
            if let Some(display) = native::macos::get_display_payload(&*view) {
                display.show_mouse(shown);
            }
        }
    }

    /// Show or hide the mouse cursor
    pub fn set_window_title(id: u16, title: String, subtitle: String) {
        let d = get_handler().lock();
        let view = unwrap_or_return!(d.get(id)).view;
        drop(d);
        // drop view as soon we have it, if let Some() keeps locked until block drop

        unsafe {
            if let Some(display) = native::macos::get_display_payload(&*view) {
                display.set_title(&title);
                display.set_subtitle(&subtitle);
            }
        }
    }

    // pub fn open_url(id: u16, url: &str) {
    //     let d = get_handler().lock();
    //     let view = unwrap_or_return!(d.get(id)).view;
    //     drop(d);
    //     unsafe {
    //         if let Some(display) = native::macos::get_display_payload(&*view) {
    //             display.open_url = url.to_owned();
    //         }
    //     }
    // }

    pub fn get_appearance() -> Appearance {
        App::appearance()
    }

    /// Set the mouse cursor icon.
    pub fn set_mouse_cursor(id: u16, cursor_icon: CursorIcon) {
        let d = get_handler().lock();
        let view = unwrap_or_return!(d.get(id)).view;
        drop(d);
        // drop view as soon we have it, if let Some() keeps locked until block drop

        unsafe {
            if let Some(display) = native::macos::get_display_payload(&*view) {
                display.set_mouse_cursor(cursor_icon);
            }
        }
    }

    /// Set the application's window size.
    pub fn set_window_size(id: u16, new_width: u32, new_height: u32) {
        let d = get_handler().lock();
        if let Some(display) = d.get(id) {
            let view = display.view;
            unsafe {
                if let Some(display) = native::macos::get_display_payload(&*view) {
                    display.set_window_size(new_width, new_height);
                }
            }
        }
    }

    pub fn set_fullscreen(id: u16, fullscreen: bool) {
        let d = get_handler().lock();
        if let Some(display) = d.get(id) {
            let view = display.view;
            unsafe {
                if let Some(display) = native::macos::get_display_payload(&*view) {
                    display.set_fullscreen(fullscreen);
                }
            }
        }
    }
    /// Get current OS clipboard value
    pub fn clipboard_get(_id: u16) -> Option<String> {
        // let mut d = get_handler().lock();
        // if let Some(d) = d.get_mut(id) {
        //     d.clipboard.get()
        // } else {
        Some(String::from(""))
        // }
    }
    /// Save value to OS clipboard
    pub fn clipboard_set(_id: u16, _data: &str) {
        // let mut d = get_handler().lock();
        // if let Some(d) = d.get_mut(id) {
        //     d.clipboard.set(data)
        // }
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

#[derive(Copy, Clone, PartialEq)]
pub enum Target {
    Game,
    Application,
}

#[cfg(target_os = "macos")]
pub type App = native::macos::App;
#[cfg(target_os = "macos")]
pub type Window = native::macos::Window;
#[cfg(target_os = "macos")]
pub type MenuItem = native::apple::menu::MenuItem;
