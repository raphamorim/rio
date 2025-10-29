// build_key_sequence was originally taken from alacritty
// which is licensed under Apache 2.0 license.

use rio_backend::crosswords::Mode;
use rio_window::event::{ElementState, KeyEvent};
use rio_window::keyboard::Key;
use rio_window::keyboard::KeyLocation;
use rio_window::keyboard::ModifiersState;
use rio_window::keyboard::NamedKey;
use rio_window::platform::modifier_supplement::KeyEventExtModifierSupplement;
use std::borrow::Cow;

#[inline(never)]
pub fn build_key_sequence(key: &KeyEvent, mods: ModifiersState, mode: Mode) -> Vec<u8> {
    let mut modifiers = mods.into();

    let kitty_seq = mode.intersects(
        Mode::REPORT_ALL_KEYS_AS_ESC
            | Mode::DISAMBIGUATE_ESC_CODES
            | Mode::REPORT_EVENT_TYPES,
    );

    let kitty_encode_all = mode.contains(Mode::REPORT_ALL_KEYS_AS_ESC);
    // The default parameter is 1, so we can omit it.
    let kitty_event_type = mode.contains(Mode::REPORT_EVENT_TYPES)
        && (key.repeat || key.state == ElementState::Released);

    let context = SequenceBuilder {
        mode,
        modifiers,
        kitty_seq,
        kitty_encode_all,
        kitty_event_type,
    };

    // Backspace whenever pressed with Fn(Globe button) on MacOS
    // will produce `\u{f728}` as text_with_all_modifiers
    // however it should act as a Delete key.
    #[cfg(target_os = "macos")]
    let text = if key.logical_key == Key::Named(NamedKey::Delete) {
        None
    } else {
        key.text_with_all_modifiers()
    };

    #[cfg(not(target_os = "macos"))]
    let text = key.text_with_all_modifiers();

    let associated_text = text.filter(|text| {
        mode.contains(Mode::REPORT_ASSOCIATED_TEXT)
            && key.state != ElementState::Released
            && !text.is_empty()
            && !is_control_character(text)
    });

    let sequence_base = context
        .try_build_numpad(key)
        .or_else(|| context.try_build_named_kitty(key))
        .or_else(|| context.try_build_named_normal(key, associated_text.is_some()))
        .or_else(|| context.try_build_control_char_or_mod(key, &mut modifiers))
        .or_else(|| context.try_build_textual(key, associated_text));

    let (payload, terminator) = match sequence_base {
        Some(SequenceBase {
            payload,
            terminator,
        }) => (payload, terminator),
        _ => return Vec::new(),
    };

    let mut payload = format!("\x1b[{payload}");

    // Add modifiers information.
    if kitty_event_type || !modifiers.is_empty() || associated_text.is_some() {
        payload.push_str(&format!(";{}", modifiers.encode_esc_sequence()));
    }

    // Push event type.
    if kitty_event_type {
        payload.push(':');
        let event_type = match key.state {
            _ if key.repeat => '2',
            ElementState::Pressed => '1',
            ElementState::Released => '3',
        };
        payload.push(event_type);
    }

    if let Some(text) = associated_text {
        let mut codepoints = text.chars().map(u32::from);
        if let Some(codepoint) = codepoints.next() {
            payload.push_str(&format!(";{codepoint}"));
        }
        for codepoint in codepoints {
            payload.push_str(&format!(":{codepoint}"));
        }
    }

    payload.push(terminator.encode_esc_sequence());

    payload.into_bytes()
}

/// Helper to build escape sequence payloads from [`KeyEvent`].
pub struct SequenceBuilder {
    mode: Mode,
    /// The emitted sequence should follow the kitty keyboard protocol.
    kitty_seq: bool,
    /// Encode all the keys according to the protocol.
    kitty_encode_all: bool,
    /// Report event types.
    kitty_event_type: bool,
    modifiers: SequenceModifiers,
}

impl SequenceBuilder {
    /// Try building sequence from the event's emitting text.
    fn try_build_textual(
        &self,
        key: &KeyEvent,
        associated_text: Option<&str>,
    ) -> Option<SequenceBase> {
        let character = match key.logical_key.as_ref() {
            Key::Character(character) if self.kitty_seq => character,
            _ => return None,
        };

        if character.chars().count() == 1 {
            let shift = self.modifiers.contains(SequenceModifiers::SHIFT);
            let ch = character.chars().next().unwrap();
            let unshifted_ch = if shift {
                ch.to_lowercase().next().unwrap()
            } else {
                ch
            };
            let alternate_key_code = u32::from(ch);
            let mut unicode_key_code = u32::from(unshifted_ch);

            // Try to get the base for keys which change based on modifier, like `1` for `!`.
            //
            // However it should only be performed when `SHIFT` is pressed.
            if shift && alternate_key_code == unicode_key_code {
                if let Key::Character(unmodded) = key.key_without_modifiers().as_ref() {
                    unicode_key_code =
                        u32::from(unmodded.chars().next().unwrap_or(unshifted_ch));
                }
            }

            // NOTE: Base layouts are ignored, since winit doesn't expose this information
            // yet.
            let payload = if self.mode.contains(Mode::REPORT_ALTERNATE_KEYS)
                && alternate_key_code != unicode_key_code
            {
                format!("{unicode_key_code}:{alternate_key_code}")
            } else {
                unicode_key_code.to_string()
            };

            Some(SequenceBase::new(payload.into(), SequenceTerminator::Kitty))
        } else if self.kitty_encode_all && associated_text.is_some() {
            // Fallback when need to report text, but we don't have any key associated with this
            // text.
            Some(SequenceBase::new("0".into(), SequenceTerminator::Kitty))
        } else {
            None
        }
    }

    /// Try building from numpad key.
    ///
    /// `None` is returned when the key is neither known nor numpad.
    fn try_build_numpad(&self, key: &KeyEvent) -> Option<SequenceBase> {
        if !self.kitty_seq || key.location != KeyLocation::Numpad {
            return None;
        }

        let base = match key.logical_key.as_ref() {
            Key::Character("0") => "57399",
            Key::Character("1") => "57400",
            Key::Character("2") => "57401",
            Key::Character("3") => "57402",
            Key::Character("4") => "57403",
            Key::Character("5") => "57404",
            Key::Character("6") => "57405",
            Key::Character("7") => "57406",
            Key::Character("8") => "57407",
            Key::Character("9") => "57408",
            Key::Character(".") => "57409",
            Key::Character("/") => "57410",
            Key::Character("*") => "57411",
            Key::Character("-") => "57412",
            Key::Character("+") => "57413",
            Key::Character("=") => "57415",
            Key::Named(named) => match named {
                NamedKey::Enter => "57414",
                NamedKey::ArrowLeft => "57417",
                NamedKey::ArrowRight => "57418",
                NamedKey::ArrowUp => "57419",
                NamedKey::ArrowDown => "57420",
                NamedKey::PageUp => "57421",
                NamedKey::PageDown => "57422",
                NamedKey::Home => "57423",
                NamedKey::End => "57424",
                NamedKey::Insert => "57425",
                NamedKey::Delete => "57426",
                _ => return None,
            },
            _ => return None,
        };

        Some(SequenceBase::new(base.into(), SequenceTerminator::Kitty))
    }

    /// Try building from [`NamedKey`] using the kitty keyboard protocol encoding
    /// for functional keys.
    fn try_build_named_kitty(&self, key: &KeyEvent) -> Option<SequenceBase> {
        let named = match key.logical_key {
            Key::Named(named) if self.kitty_seq => named,
            _ => return None,
        };

        let (base, terminator) = match named {
            // F3 in kitty protocol diverges from alacritty's terminfo.
            NamedKey::F3 => ("13", SequenceTerminator::Normal('~')),
            NamedKey::F13 => ("57376", SequenceTerminator::Kitty),
            NamedKey::F14 => ("57377", SequenceTerminator::Kitty),
            NamedKey::F15 => ("57378", SequenceTerminator::Kitty),
            NamedKey::F16 => ("57379", SequenceTerminator::Kitty),
            NamedKey::F17 => ("57380", SequenceTerminator::Kitty),
            NamedKey::F18 => ("57381", SequenceTerminator::Kitty),
            NamedKey::F19 => ("57382", SequenceTerminator::Kitty),
            NamedKey::F20 => ("57383", SequenceTerminator::Kitty),
            NamedKey::F21 => ("57384", SequenceTerminator::Kitty),
            NamedKey::F22 => ("57385", SequenceTerminator::Kitty),
            NamedKey::F23 => ("57386", SequenceTerminator::Kitty),
            NamedKey::F24 => ("57387", SequenceTerminator::Kitty),
            NamedKey::F25 => ("57388", SequenceTerminator::Kitty),
            NamedKey::F26 => ("57389", SequenceTerminator::Kitty),
            NamedKey::F27 => ("57390", SequenceTerminator::Kitty),
            NamedKey::F28 => ("57391", SequenceTerminator::Kitty),
            NamedKey::F29 => ("57392", SequenceTerminator::Kitty),
            NamedKey::F30 => ("57393", SequenceTerminator::Kitty),
            NamedKey::F31 => ("57394", SequenceTerminator::Kitty),
            NamedKey::F32 => ("57395", SequenceTerminator::Kitty),
            NamedKey::F33 => ("57396", SequenceTerminator::Kitty),
            NamedKey::F34 => ("57397", SequenceTerminator::Kitty),
            NamedKey::F35 => ("57398", SequenceTerminator::Kitty),
            NamedKey::ScrollLock => ("57359", SequenceTerminator::Kitty),
            NamedKey::PrintScreen => ("57361", SequenceTerminator::Kitty),
            NamedKey::Pause => ("57362", SequenceTerminator::Kitty),
            NamedKey::ContextMenu => ("57363", SequenceTerminator::Kitty),
            NamedKey::MediaPlay => ("57428", SequenceTerminator::Kitty),
            NamedKey::MediaPause => ("57429", SequenceTerminator::Kitty),
            NamedKey::MediaPlayPause => ("57430", SequenceTerminator::Kitty),
            NamedKey::MediaStop => ("57432", SequenceTerminator::Kitty),
            NamedKey::MediaFastForward => ("57433", SequenceTerminator::Kitty),
            NamedKey::MediaRewind => ("57434", SequenceTerminator::Kitty),
            NamedKey::MediaTrackNext => ("57435", SequenceTerminator::Kitty),
            NamedKey::MediaTrackPrevious => ("57436", SequenceTerminator::Kitty),
            NamedKey::MediaRecord => ("57437", SequenceTerminator::Kitty),
            NamedKey::AudioVolumeDown => ("57438", SequenceTerminator::Kitty),
            NamedKey::AudioVolumeUp => ("57439", SequenceTerminator::Kitty),
            NamedKey::AudioVolumeMute => ("57440", SequenceTerminator::Kitty),
            _ => return None,
        };

        Some(SequenceBase::new(base.into(), terminator))
    }

    /// Try building from [`NamedKey`].
    fn try_build_named_normal(
        &self,
        key: &KeyEvent,
        has_associated_text: bool,
    ) -> Option<SequenceBase> {
        let named = match key.logical_key {
            Key::Named(named) => named,
            _ => return None,
        };

        // The default parameter is 1, so we can omit it.
        let one_based = if self.modifiers.is_empty()
            && !self.kitty_event_type
            && !has_associated_text
        {
            ""
        } else {
            "1"
        };
        let (base, terminator) = match named {
            NamedKey::PageUp => ("5", SequenceTerminator::Normal('~')),
            NamedKey::PageDown => ("6", SequenceTerminator::Normal('~')),
            NamedKey::Insert => ("2", SequenceTerminator::Normal('~')),
            NamedKey::Delete => ("3", SequenceTerminator::Normal('~')),
            NamedKey::Home => (one_based, SequenceTerminator::Normal('H')),
            NamedKey::End => (one_based, SequenceTerminator::Normal('F')),
            NamedKey::ArrowLeft => (one_based, SequenceTerminator::Normal('D')),
            NamedKey::ArrowRight => (one_based, SequenceTerminator::Normal('C')),
            NamedKey::ArrowUp => (one_based, SequenceTerminator::Normal('A')),
            NamedKey::ArrowDown => (one_based, SequenceTerminator::Normal('B')),
            NamedKey::F1 => (one_based, SequenceTerminator::Normal('P')),
            NamedKey::F2 => (one_based, SequenceTerminator::Normal('Q')),
            NamedKey::F3 => (one_based, SequenceTerminator::Normal('R')),
            NamedKey::F4 => (one_based, SequenceTerminator::Normal('S')),
            NamedKey::F5 => ("15", SequenceTerminator::Normal('~')),
            NamedKey::F6 => ("17", SequenceTerminator::Normal('~')),
            NamedKey::F7 => ("18", SequenceTerminator::Normal('~')),
            NamedKey::F8 => ("19", SequenceTerminator::Normal('~')),
            NamedKey::F9 => ("20", SequenceTerminator::Normal('~')),
            NamedKey::F10 => ("21", SequenceTerminator::Normal('~')),
            NamedKey::F11 => ("23", SequenceTerminator::Normal('~')),
            NamedKey::F12 => ("24", SequenceTerminator::Normal('~')),
            NamedKey::F13 => ("25", SequenceTerminator::Normal('~')),
            NamedKey::F14 => ("26", SequenceTerminator::Normal('~')),
            NamedKey::F15 => ("28", SequenceTerminator::Normal('~')),
            NamedKey::F16 => ("29", SequenceTerminator::Normal('~')),
            NamedKey::F17 => ("31", SequenceTerminator::Normal('~')),
            NamedKey::F18 => ("32", SequenceTerminator::Normal('~')),
            NamedKey::F19 => ("33", SequenceTerminator::Normal('~')),
            NamedKey::F20 => ("34", SequenceTerminator::Normal('~')),
            _ => return None,
        };

        Some(SequenceBase::new(base.into(), terminator))
    }

    /// Try building escape from control characters (e.g. Enter) and modifiers.
    fn try_build_control_char_or_mod(
        &self,
        key: &KeyEvent,
        mods: &mut SequenceModifiers,
    ) -> Option<SequenceBase> {
        if !self.kitty_encode_all && !self.kitty_seq {
            return None;
        }

        let named = match key.logical_key {
            Key::Named(named) => named,
            _ => return None,
        };

        let base = match named {
            NamedKey::Tab => "9",
            NamedKey::Enter => "13",
            NamedKey::Escape => "27",
            NamedKey::Space => "32",
            NamedKey::Backspace => "127",
            _ => "",
        };

        // Fail when the key is not a named control character and the active mode prohibits us
        // from encoding modifier keys.
        if !self.kitty_encode_all && base.is_empty() {
            return None;
        }

        let base = match (named, key.location) {
            (NamedKey::Shift, KeyLocation::Left) => "57441",
            (NamedKey::Control, KeyLocation::Left) => "57442",
            (NamedKey::Alt, KeyLocation::Left) => "57443",
            (NamedKey::Super, KeyLocation::Left) => "57444",
            (NamedKey::Hyper, KeyLocation::Left) => "57445",
            (NamedKey::Meta, KeyLocation::Left) => "57446",
            (NamedKey::Shift, _) => "57447",
            (NamedKey::Control, _) => "57448",
            (NamedKey::Alt, _) => "57449",
            (NamedKey::Super, _) => "57450",
            (NamedKey::Hyper, _) => "57451",
            (NamedKey::Meta, _) => "57452",
            (NamedKey::CapsLock, _) => "57358",
            (NamedKey::NumLock, _) => "57360",
            _ => base,
        };

        // NOTE: Kitty's protocol mandates that the modifier state is applied before
        // key press, however winit sends them after the key press, so for modifiers
        // itself apply the state based on keysyms and not the _actual_ modifiers
        // state, which is how kitty is doing so and what is suggested in such case.
        let press = key.state.is_pressed();
        match named {
            NamedKey::Shift => mods.set(SequenceModifiers::SHIFT, press),
            NamedKey::Control => mods.set(SequenceModifiers::CONTROL, press),
            NamedKey::Alt => mods.set(SequenceModifiers::ALT, press),
            NamedKey::Super => mods.set(SequenceModifiers::SUPER, press),
            _ => (),
        }

        if base.is_empty() {
            None
        } else {
            Some(SequenceBase::new(base.into(), SequenceTerminator::Kitty))
        }
    }
}

pub struct SequenceBase {
    /// The base of the payload, which is the `number` and optionally an alt base from the kitty
    /// spec.
    payload: Cow<'static, str>,
    terminator: SequenceTerminator,
}

impl SequenceBase {
    fn new(payload: Cow<'static, str>, terminator: SequenceTerminator) -> Self {
        Self {
            payload,
            terminator,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceTerminator {
    /// The normal key esc sequence terminator defined by xterm/dec.
    Normal(char),
    /// The terminator is for kitty escape sequence.
    Kitty,
}

impl SequenceTerminator {
    fn encode_esc_sequence(self) -> char {
        match self {
            SequenceTerminator::Normal(char) => char,
            SequenceTerminator::Kitty => 'u',
        }
    }
}

bitflags::bitflags! {
    /// The modifiers encoding for escape sequence.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SequenceModifiers : u8 {
        const SHIFT   = 0b0000_0001;
        const ALT     = 0b0000_0010;
        const CONTROL = 0b0000_0100;
        const SUPER   = 0b0000_1000;
        // NOTE: Kitty protocol defines additional modifiers to what is present here, like
        // Capslock, but it's not a modifier as per winit.
    }
}

impl SequenceModifiers {
    /// Get the value which should be passed to escape sequence.
    pub fn encode_esc_sequence(self) -> u8 {
        self.bits() + 1
    }
}

impl From<ModifiersState> for SequenceModifiers {
    fn from(mods: ModifiersState) -> Self {
        let mut modifiers = Self::empty();
        modifiers.set(Self::SHIFT, mods.shift_key());
        modifiers.set(Self::ALT, mods.alt_key());
        modifiers.set(Self::CONTROL, mods.control_key());
        modifiers.set(Self::SUPER, mods.super_key());
        modifiers
    }
}

/// Check whether the `text` is `0x7f`, `C0` or `C1` control code.
fn is_control_character(text: &str) -> bool {
    // 0x7f (DEL) is included here since it has a dedicated control code (`^?`) which generally
    // does not match the reported text (`^H`), despite not technically being part of C0 or C1.
    let codepoint = text.bytes().next().unwrap();
    text.len() == 1 && (codepoint < 0x20 || (0x7f..=0x9f).contains(&codepoint))
}
