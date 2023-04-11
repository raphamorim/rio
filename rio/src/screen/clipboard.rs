#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use std::ffi::c_void;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardType {
    Clipboard,
    Selection,
}

#[cfg(any(test, not(any(feature = "x11", target_os = "macos", windows))))]
use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use copypasta::wayland_clipboard;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use copypasta::x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext};
#[cfg(any(feature = "x11", target_os = "macos", windows))]
use copypasta::ClipboardContext;
use copypasta::ClipboardProvider;

pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
}

impl Clipboard {
    #[cfg(any(not(feature = "wayland"), target_os = "macos", windows))]
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
    pub unsafe fn new(display: Option<*mut c_void>) -> Self {
        match display {
            Some(display) => {
                let (selection, clipboard) =
                    wayland_clipboard::create_clipboards_from_external(display);
                Self {
                    clipboard: Box::new(clipboard),
                    selection: Some(Box::new(selection)),
                }
            }
            None => Self::default(),
        }
    }

    /// Used for tests and to handle missing clipboard provider when built without the `x11`
    /// feature.
    #[allow(unused)]
    #[cfg(any(test, not(any(feature = "x11", target_os = "macos", windows))))]
    pub fn new_nop() -> Self {
        Self {
            clipboard: Box::new(NopClipboardContext::new().unwrap()),
            selection: None,
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", windows))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: None,
        };

        #[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
        return Self {
            clipboard: Box::new(ClipboardContext::new().unwrap()),
            selection: Some(Box::new(
                X11ClipboardContext::<X11SelectionClipboard>::new().unwrap(),
            )),
        };

        #[cfg(not(any(feature = "x11", target_os = "macos", windows)))]
        return Self::new_nop();
    }
}

impl Clipboard {
    pub fn set(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            (ClipboardType::Selection, None) => return,
            _ => &mut self.clipboard,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            println!("Unable to store text in clipboard: {}", err);
        });
    }

    pub fn get(&mut self, ty: ClipboardType) -> String {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            _ => &mut self.clipboard,
        };

        match clipboard.get_contents() {
            Err(err) => {
                println!("Unable to load text from clipboard: {}", err);
                String::new()
            }
            Ok(text) => text,
        }
    }
}
