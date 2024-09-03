use std::sync::{
    atomic::{AtomicBool, Ordering::Relaxed},
    Mutex,
};

use winapi::{
    shared::{
        minwindef::{LPARAM, WPARAM},
        windef::HWND,
    },
    um::winuser,
};

use crate::platform_impl::platform::{event_loop::ProcResult, keyboard::next_kbd_msg};

pub struct MinimalIme {
    // True if we're currently receiving messages belonging to a finished IME session.
    getting_ime_text: AtomicBool,

    utf16parts: Mutex<Vec<u16>>,
}
impl Default for MinimalIme {
    fn default() -> Self {
        MinimalIme {
            getting_ime_text: AtomicBool::new(false),
            utf16parts: Mutex::new(Vec::with_capacity(16)),
        }
    }
}
impl MinimalIme {
    pub(crate) fn process_message(
        &self,
        hwnd: HWND,
        msg_kind: u32,
        wparam: WPARAM,
        _lparam: LPARAM,
        result: &mut ProcResult,
    ) -> Option<String> {
        match msg_kind {
            winuser::WM_IME_ENDCOMPOSITION => {
                self.getting_ime_text.store(true, Relaxed);
            }
            winuser::WM_CHAR | winuser::WM_SYSCHAR => {
                if self.getting_ime_text.load(Relaxed) {
                    *result = ProcResult::Value(0);
                    self.utf16parts.lock().unwrap().push(wparam as u16);
                    // It's important that we push the new character and release the lock
                    // before getting the next message
                    let next_msg = next_kbd_msg(hwnd);
                    let more_char_coming = next_msg
                        .map(|m| matches!(m.message, winuser::WM_CHAR | winuser::WM_SYSCHAR))
                        .unwrap_or(false);
                    if !more_char_coming {
                        let mut utf16parts = self.utf16parts.lock().unwrap();
                        let result = String::from_utf16(&utf16parts).ok();
                        utf16parts.clear();
                        self.getting_ime_text.store(false, Relaxed);
                        return result;
                    }
                }
            }
            _ => (),
        }

        None
    }
}
