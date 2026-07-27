// clipboard.rs was retired originally from https://github.com/alacritty/alacritty/blob/e35e5ad14fce8456afdd89f2b392b9924bb27471/alacritty/src/clipboard.rs
// which is licensed under Apache 2.0 license.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardType {
    Clipboard,
    Selection,
}

// The `Clipboard` helper below backs onto the system clipboard via copypasta,
// which pulls in windowing/AppKit dependencies. It is only used by frontends;
// headless embedders (parse + snapshot, OSC 52 as events) need just the
// `ClipboardType` enum above, so the rest is gated behind the `clipboard`
// feature.
#[cfg(feature = "clipboard")]
use raw_window_handle::RawDisplayHandle;
#[cfg(feature = "clipboard")]
use tracing::warn;

#[cfg(feature = "clipboard")]
use copypasta::nop_clipboard::NopClipboardContext;
#[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
use copypasta::wayland_clipboard;
#[cfg(all(feature = "x11", not(any(target_os = "macos", windows))))]
use copypasta::x11_clipboard::{Primary as X11SelectionClipboard, X11ClipboardContext};
#[cfg(all(feature = "clipboard", any(feature = "x11", target_os = "macos", windows)))]
use copypasta::ClipboardContext;
#[cfg(feature = "clipboard")]
use copypasta::ClipboardProvider;

#[cfg(feature = "clipboard")]
pub struct Clipboard {
    clipboard: Box<dyn ClipboardProvider>,
    selection: Option<Box<dyn ClipboardProvider>>,
}

#[cfg(feature = "clipboard")]
impl Clipboard {
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn new(display: RawDisplayHandle) -> Self {
        match display {
            #[cfg(all(feature = "wayland", not(any(target_os = "macos", windows))))]
            RawDisplayHandle::Wayland(display) => {
                let (selection, clipboard) =
                    wayland_clipboard::create_clipboards_from_external(
                        display.display.as_ptr(),
                    );
                Self {
                    clipboard: Box::new(clipboard),
                    selection: Some(Box::new(selection)),
                }
            }
            _ => Self::default(),
        }
    }

    /// Used for tests and to handle missing clipboard provider when built without the `x11`
    /// feature.
    pub fn new_nop() -> Self {
        Self {
            clipboard: Box::new(NopClipboardContext::new().unwrap()),
            selection: None,
        }
    }
}

#[cfg(feature = "clipboard")]
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

#[cfg(feature = "clipboard")]
impl Clipboard {
    pub fn set(&mut self, ty: ClipboardType, text: impl Into<String>) {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            (ClipboardType::Selection, None) => return,
            _ => &mut self.clipboard,
        };

        clipboard.set_contents(text.into()).unwrap_or_else(|err| {
            warn!("Unable to store text in clipboard: {}", err);
        });
    }

    pub fn get(&mut self, ty: ClipboardType) -> String {
        let clipboard = match (ty, &mut self.selection) {
            (ClipboardType::Selection, Some(provider)) => provider,
            _ => &mut self.clipboard,
        };

        match clipboard.get_contents() {
            Err(err) => {
                warn!("Unable to load text from clipboard: {}", err);
                String::new()
            }
            Ok(text) => text,
        }
    }
}
