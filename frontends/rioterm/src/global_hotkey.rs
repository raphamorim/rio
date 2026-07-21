//! System-wide hotkey registration for the quake window. Uses the
//! `global-hotkey` crate: Carbon `RegisterEventHotKey` on macOS (no
//! accessibility permission needed), `RegisterHotKey` on Windows and
//! `XGrabKey` on X11. Pure Wayland has no global hotkey API; users
//! bind a compositor key to a regular Rio binding instead.

use crate::event::EventProxy;
use global_hotkey::{hotkey::HotKey, GlobalHotKeyManager};
use rio_backend::event::{RioEvent, RioEventType};

/// Keeps the OS hotkey registrations alive for the app's lifetime.
pub struct GlobalHotkeys {
    _manager: GlobalHotKeyManager,
}

/// Register a system-wide hotkey for every `ToggleQuake` binding.
/// Nothing is created when no binding exists or none of the triggers
/// parse: the manager itself owns OS resources (a Carbon handler on
/// macOS, a hidden window on Windows, an X connection thread on X11)
/// that a quake-less config should never pay for.
pub fn setup(
    event_proxy: EventProxy,
    keys: &[rio_backend::config::bindings::KeyBinding],
) -> Option<GlobalHotkeys> {
    let hotkeys: Vec<(String, HotKey)> = quake_triggers(keys)
        .into_iter()
        .filter_map(|trigger| match parse_hotkey(&trigger) {
            Ok(hotkey) => Some((trigger, hotkey)),
            Err(err) => {
                tracing::warn!("quake hotkey '{trigger}': {err}");
                None
            }
        })
        .collect();
    if hotkeys.is_empty() {
        return None;
    }

    let manager = match GlobalHotKeyManager::new() {
        Ok(manager) => manager,
        Err(err) => {
            tracing::warn!("global hotkeys unavailable: {err}");
            return None;
        }
    };

    let mut registered = false;
    for (trigger, hotkey) in hotkeys {
        match manager.register(hotkey) {
            Ok(()) => {
                registered = true;
                tracing::info!("registered global hotkey: {trigger}");
            }
            Err(err) => tracing::warn!("quake hotkey '{trigger}': {err}"),
        }
    }
    if !registered {
        return None;
    }

    std::thread::spawn(move || {
        use global_hotkey::{GlobalHotKeyEvent, HotKeyState};
        while let Ok(event) = GlobalHotKeyEvent::receiver().recv() {
            if event.state == HotKeyState::Pressed {
                event_proxy
                    .send_event(RioEventType::Rio(RioEvent::ToggleQuake), unsafe {
                        rio_window::window::WindowId::dummy()
                    });
            }
        }
    });

    Some(GlobalHotkeys { _manager: manager })
}

/// Triggers for every `ToggleQuake` binding in the config, in the
/// format `parse_hotkey` accepts.
pub fn quake_triggers(keys: &[rio_backend::config::bindings::KeyBinding]) -> Vec<String> {
    keys.iter()
        .filter(|binding| binding.action.to_lowercase() == "togglequake")
        .map(|binding| {
            let mods = binding.with.replace(' ', "").replace('|', "+");
            if mods.is_empty() {
                binding.key.clone()
            } else {
                format!("{}+{}", mods, binding.key)
            }
        })
        .collect()
}

/// Parse a binding-style trigger ("super+shift+q", "f12") into a
/// `HotKey`. Accepts the same modifier names as `[bindings]` `with`.
fn parse_hotkey(trigger: &str) -> Result<HotKey, String> {
    use global_hotkey::hotkey::{Code, Modifiers};

    let lowered = trigger.to_lowercase();
    let mut modifiers = Modifiers::empty();
    let mut code = None;

    for part in lowered.split('+') {
        match part.trim() {
            "cmd" | "super" | "command" => modifiers |= Modifiers::SUPER,
            "ctrl" | "control" => modifiers |= Modifiers::CONTROL,
            "alt" | "option" => modifiers |= Modifiers::ALT,
            "shift" => modifiers |= Modifiers::SHIFT,
            // The bindings parser accepts "none" as a no-op modifier.
            "none" => {}
            "escape" | "esc" => code = Some(Code::Escape),
            "space" => code = Some(Code::Space),
            "enter" | "return" => code = Some(Code::Enter),
            "tab" => code = Some(Code::Tab),
            "back" | "backspace" => code = Some(Code::Backspace),
            "delete" => code = Some(Code::Delete),
            "insert" => code = Some(Code::Insert),
            "home" => code = Some(Code::Home),
            "end" => code = Some(Code::End),
            "pageup" => code = Some(Code::PageUp),
            "pagedown" => code = Some(Code::PageDown),
            "up" => code = Some(Code::ArrowUp),
            "down" => code = Some(Code::ArrowDown),
            "left" => code = Some(Code::ArrowLeft),
            "right" => code = Some(Code::ArrowRight),
            "f1" => code = Some(Code::F1),
            "f2" => code = Some(Code::F2),
            "f3" => code = Some(Code::F3),
            "f4" => code = Some(Code::F4),
            "f5" => code = Some(Code::F5),
            "f6" => code = Some(Code::F6),
            "f7" => code = Some(Code::F7),
            "f8" => code = Some(Code::F8),
            "f9" => code = Some(Code::F9),
            "f10" => code = Some(Code::F10),
            "f11" => code = Some(Code::F11),
            "f12" => code = Some(Code::F12),
            "`" | "grave" | "backquote" => code = Some(Code::Backquote),
            "'" | "quote" => code = Some(Code::Quote),
            "," | "comma" => code = Some(Code::Comma),
            "." | "period" => code = Some(Code::Period),
            "/" | "slash" => code = Some(Code::Slash),
            "\\" | "backslash" => code = Some(Code::Backslash),
            ";" | "semicolon" => code = Some(Code::Semicolon),
            "-" | "minus" => code = Some(Code::Minus),
            "=" | "equal" => code = Some(Code::Equal),
            "[" | "bracketleft" => code = Some(Code::BracketLeft),
            "]" | "bracketright" => code = Some(Code::BracketRight),
            "numpadenter" => code = Some(Code::NumpadEnter),
            "numpadadd" => code = Some(Code::NumpadAdd),
            "numpadsubtract" => code = Some(Code::NumpadSubtract),
            "numpadmultiply" => code = Some(Code::NumpadMultiply),
            "numpaddivide" => code = Some(Code::NumpadDivide),
            "numpaddecimal" => code = Some(Code::NumpadDecimal),
            "numpadcomma" => code = Some(Code::NumpadComma),
            "numpadequals" => code = Some(Code::NumpadEqual),
            "numpad0" => code = Some(Code::Numpad0),
            "numpad1" => code = Some(Code::Numpad1),
            "numpad2" => code = Some(Code::Numpad2),
            "numpad3" => code = Some(Code::Numpad3),
            "numpad4" => code = Some(Code::Numpad4),
            "numpad5" => code = Some(Code::Numpad5),
            "numpad6" => code = Some(Code::Numpad6),
            "numpad7" => code = Some(Code::Numpad7),
            "numpad8" => code = Some(Code::Numpad8),
            "numpad9" => code = Some(Code::Numpad9),
            key if key.len() == 1 => {
                let ch = key.chars().next().unwrap();
                code = Some(match ch {
                    'a'..='z' => letter_code(ch),
                    '0'..='9' => digit_code(ch),
                    _ => return Err(format!("unsupported key '{key}'")),
                });
            }
            other => return Err(format!("unknown key or modifier '{other}'")),
        }
    }

    let code = code.ok_or_else(|| "no key in trigger".to_string())?;
    Ok(HotKey::new(
        (!modifiers.is_empty()).then_some(modifiers),
        code,
    ))
}

fn letter_code(ch: char) -> global_hotkey::hotkey::Code {
    use global_hotkey::hotkey::Code::*;
    match ch {
        'a' => KeyA,
        'b' => KeyB,
        'c' => KeyC,
        'd' => KeyD,
        'e' => KeyE,
        'f' => KeyF,
        'g' => KeyG,
        'h' => KeyH,
        'i' => KeyI,
        'j' => KeyJ,
        'k' => KeyK,
        'l' => KeyL,
        'm' => KeyM,
        'n' => KeyN,
        'o' => KeyO,
        'p' => KeyP,
        'q' => KeyQ,
        'r' => KeyR,
        's' => KeyS,
        't' => KeyT,
        'u' => KeyU,
        'v' => KeyV,
        'w' => KeyW,
        'x' => KeyX,
        'y' => KeyY,
        'z' => KeyZ,
        _ => unreachable!(),
    }
}

fn digit_code(ch: char) -> global_hotkey::hotkey::Code {
    use global_hotkey::hotkey::Code::*;
    match ch {
        '0' => Digit0,
        '1' => Digit1,
        '2' => Digit2,
        '3' => Digit3,
        '4' => Digit4,
        '5' => Digit5,
        '6' => Digit6,
        '7' => Digit7,
        '8' => Digit8,
        '9' => Digit9,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::{Code, Modifiers};

    #[test]
    fn parse_hotkey_forms() {
        let hk = parse_hotkey("super+shift+q").unwrap();
        assert_eq!(
            hk,
            HotKey::new(Some(Modifiers::SUPER | Modifiers::SHIFT), Code::KeyQ)
        );
        let hk = parse_hotkey("control+`").unwrap();
        assert_eq!(hk, HotKey::new(Some(Modifiers::CONTROL), Code::Backquote));
        let hk = parse_hotkey("f12").unwrap();
        assert_eq!(hk, HotKey::new(None, Code::F12));
        assert!(parse_hotkey("super+banana").is_err());
        assert!(parse_hotkey("super+shift").is_err());
        // Parity with the bindings parser.
        let hk = parse_hotkey("none+f12").unwrap();
        assert_eq!(hk, HotKey::new(None, Code::F12));
        let hk = parse_hotkey("control+numpad5").unwrap();
        assert_eq!(hk, HotKey::new(Some(Modifiers::CONTROL), Code::Numpad5));
        let hk = parse_hotkey("shift+insert").unwrap();
        assert_eq!(hk, HotKey::new(Some(Modifiers::SHIFT), Code::Insert));
    }

    #[test]
    fn quake_triggers_from_bindings() {
        use rio_backend::config::bindings::KeyBinding;
        let binding = |key: &str, with: &str, action: &str| KeyBinding {
            key: key.into(),
            with: with.into(),
            action: action.into(),
            esc: String::new(),
            mode: String::new(),
        };
        let keys = vec![
            binding("q", "super | shift", "ToggleQuake"),
            binding("f12", "", "togglequake"),
            binding("f12", "none", "togglequake"),
            binding("w", "super", "quit"),
        ];
        let triggers = quake_triggers(&keys);
        assert_eq!(triggers, vec!["super+shift+q", "f12", "none+f12"]);
        for trigger in &triggers {
            assert!(parse_hotkey(trigger).is_ok(), "{trigger} must parse");
        }
    }
}
