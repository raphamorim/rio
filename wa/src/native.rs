// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Originally retired from https://github.com/not-fl3/macroquad licensed under MIT
// https://github.com/not-fl3/macroquad/blob/master/LICENSE-MIT
// The code has suffered several changes like support to multiple windows, extension of windows
// properties, menu support, IME support, and etc.

// use std::collections::HashMap;
use fnv::FnvHashMap;

// let mut map = FnvHashMap::default();

pub(crate) struct Handler {
    inner: FnvHashMap<u16, NativeDisplayData>,
    next: u16,
}

impl Handler {
    pub fn new() -> Self {
        Handler {
            inner: FnvHashMap::default(),
            next: 0,
        }
    }

    #[inline]
    pub fn insert(&mut self, id: u16, display: NativeDisplayData) {
        self.inner.insert(id, display);
    }

    #[inline]
    pub fn get_mut(&mut self, id: u16) -> Option<&mut NativeDisplayData> {
        self.inner.get_mut(&id)
    }

    #[inline]
    pub fn get(&self, id: u16) -> Option<&NativeDisplayData> {
        self.inner.get(&id)
    }

    #[inline]
    pub fn next_id(&mut self) -> u16 {
        let next = self.next;
        self.next += 1;
        next
    }

    #[inline]
    #[allow(dead_code)]
    pub fn remove(&mut self, id: u16) {
        self.inner.remove(&id);
    }
}

pub(crate) struct NativeDisplayData {
    pub quit_requested: bool,
    pub quit_ordered: bool,
    // pub clipboard: Box<dyn Clipboard>,
    pub display_handle: Option<raw_window_handle::RawDisplayHandle>,
    pub window_handle: Option<raw_window_handle::RawWindowHandle>,

    #[cfg(target_vendor = "apple")]
    pub view: crate::native::apple::frameworks::ObjcId,
}
#[cfg(target_vendor = "apple")]
unsafe impl Send for NativeDisplayData {}
#[cfg(target_vendor = "apple")]
unsafe impl Sync for NativeDisplayData {}

impl NativeDisplayData {
    pub fn new(// clipboard: Box<dyn Clipboard>,
    ) -> NativeDisplayData {
        NativeDisplayData {
            quit_requested: false,
            quit_ordered: false,
            // clipboard,
            display_handle: None,
            window_handle: None,
            #[cfg(target_vendor = "apple")]
            view: std::ptr::null_mut(),
        }
    }
}

pub trait Clipboard: Send + Sync {
    fn get(&mut self) -> Option<String>;
    fn set(&mut self, string: &str);
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
pub mod apple;

#[cfg(target_os = "macos")]
pub mod macos;
