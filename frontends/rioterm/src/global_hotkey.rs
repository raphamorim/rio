use crate::event::EventProxy;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager as GHKManager};
use rio_backend::event::{RioEvent, RioEventType};
use rio_window::window::WindowId;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

static EVENT_LISTENER_STARTED: AtomicBool = AtomicBool::new(false);

pub struct GlobalHotkeyManager {
    manager: GHKManager,
    event_proxy: EventProxy,
    window_id: WindowId,
    hotkeys: HashMap<u32, HotKey>,
}

impl GlobalHotkeyManager {
    pub fn new(event_proxy: EventProxy, window_id: WindowId) -> Self {
        Self {
            manager: GHKManager::new().expect("Failed to create global hotkey manager"),
            event_proxy,
            window_id,
            hotkeys: HashMap::new(),
        }
    }

    pub fn register_hotkey(&mut self, hotkey_string: &str) -> Result<(), String> {
        // Parse the hotkey string (e.g., "cmd+shift+escape")
        let hotkey = self.parse_hotkey_string(hotkey_string)?;

        // Register the hotkey
        self.manager
            .register(hotkey)
            .map_err(|e| format!("Failed to register hotkey: {}", e))?;

        // Store the hotkey for later unregistration
        self.hotkeys.insert(hotkey.id(), hotkey);

        tracing::info!("Successfully registered global hotkey: {}", hotkey_string);

        // Start listening for hotkey events in a separate thread (only once)
        if !EVENT_LISTENER_STARTED.load(Ordering::Relaxed) {
            EVENT_LISTENER_STARTED.store(true, Ordering::Relaxed);
            self.start_event_listener();
        }

        Ok(())
    }

    pub fn unregister_hotkey(&mut self) {
        for (_, hotkey) in self.hotkeys.drain() {
            if let Err(e) = self.manager.unregister(hotkey) {
                tracing::warn!("Failed to unregister hotkey: {}", e);
            }
        }
    }

    fn parse_hotkey_string(&self, hotkey_string: &str) -> Result<HotKey, String> {
        use global_hotkey::hotkey::{Code, Modifiers};

        let lowercase_string = hotkey_string.to_lowercase();
        let parts: Vec<&str> = lowercase_string.split('+').collect();
        let mut modifiers = Modifiers::empty();
        let mut key_code = None;

        for part in parts {
            match part.trim() {
                "cmd" | "super" => modifiers |= Modifiers::SUPER,
                "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
                "alt" | "option" => modifiers |= Modifiers::ALT,
                "shift" => modifiers |= Modifiers::SHIFT,
                "escape" | "esc" => key_code = Some(Code::Escape),
                "space" => key_code = Some(Code::Space),
                "enter" | "return" => key_code = Some(Code::Enter),
                "tab" => key_code = Some(Code::Tab),
                "backspace" => key_code = Some(Code::Backspace),
                "delete" => key_code = Some(Code::Delete),
                "f1" => key_code = Some(Code::F1),
                "f2" => key_code = Some(Code::F2),
                "f3" => key_code = Some(Code::F3),
                "f4" => key_code = Some(Code::F4),
                "f5" => key_code = Some(Code::F5),
                "f6" => key_code = Some(Code::F6),
                "f7" => key_code = Some(Code::F7),
                "f8" => key_code = Some(Code::F8),
                "f9" => key_code = Some(Code::F9),
                "f10" => key_code = Some(Code::F10),
                "f11" => key_code = Some(Code::F11),
                "f12" => key_code = Some(Code::F12),
                // Single character keys
                key if key.len() == 1 => {
                    let ch = key.chars().next().unwrap();
                    match ch {
                        'a' => key_code = Some(Code::KeyA),
                        'b' => key_code = Some(Code::KeyB),
                        'c' => key_code = Some(Code::KeyC),
                        'd' => key_code = Some(Code::KeyD),
                        'e' => key_code = Some(Code::KeyE),
                        'f' => key_code = Some(Code::KeyF),
                        'g' => key_code = Some(Code::KeyG),
                        'h' => key_code = Some(Code::KeyH),
                        'i' => key_code = Some(Code::KeyI),
                        'j' => key_code = Some(Code::KeyJ),
                        'k' => key_code = Some(Code::KeyK),
                        'l' => key_code = Some(Code::KeyL),
                        'm' => key_code = Some(Code::KeyM),
                        'n' => key_code = Some(Code::KeyN),
                        'o' => key_code = Some(Code::KeyO),
                        'p' => key_code = Some(Code::KeyP),
                        'q' => key_code = Some(Code::KeyQ),
                        'r' => key_code = Some(Code::KeyR),
                        's' => key_code = Some(Code::KeyS),
                        't' => key_code = Some(Code::KeyT),
                        'u' => key_code = Some(Code::KeyU),
                        'v' => key_code = Some(Code::KeyV),
                        'w' => key_code = Some(Code::KeyW),
                        'x' => key_code = Some(Code::KeyX),
                        'y' => key_code = Some(Code::KeyY),
                        'z' => key_code = Some(Code::KeyZ),
                        '0' => key_code = Some(Code::Digit0),
                        '1' => key_code = Some(Code::Digit1),
                        '2' => key_code = Some(Code::Digit2),
                        '3' => key_code = Some(Code::Digit3),
                        '4' => key_code = Some(Code::Digit4),
                        '5' => key_code = Some(Code::Digit5),
                        '6' => key_code = Some(Code::Digit6),
                        '7' => key_code = Some(Code::Digit7),
                        '8' => key_code = Some(Code::Digit8),
                        '9' => key_code = Some(Code::Digit9),
                        _ => return Err(format!("Unsupported key: {}", key)),
                    }
                }
                _ => return Err(format!("Unknown key or modifier: {}", part)),
            }
        }

        let code = key_code.ok_or_else(|| "No key code specified".to_string())?;
        Ok(HotKey::new(Some(modifiers), code))
    }

    fn start_event_listener(&self) {
        let event_proxy = self.event_proxy.clone();
        let window_id = self.window_id;

        std::thread::spawn(move || {
            use global_hotkey::GlobalHotKeyEvent;

            loop {
                if let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
                    match event.state {
                        global_hotkey::HotKeyState::Pressed => {
                            tracing::info!("Global hotkey pressed: {}", event.id);
                            event_proxy.send_event(
                                RioEventType::Rio(RioEvent::QuakeGlobalHotkey),
                                window_id,
                            );
                        }
                        global_hotkey::HotKeyState::Released => {
                            // We don't need to handle key release for quake mode
                        }
                    }
                }

                // Small sleep to prevent busy waiting
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });
    }
}

impl Drop for GlobalHotkeyManager {
    fn drop(&mut self) {
        self.unregister_hotkey();
    }
}
