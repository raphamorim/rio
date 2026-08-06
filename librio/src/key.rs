//! Key event encoding.
//!
//! An embedder hands over the platform's key event more or less as it arrived,
//! including the text the platform produced for it, and this module decides
//! what reaches the pty. The decision needs terminal state (application cursor
//! mode, the kitty keyboard flags, `modifyOtherKeys`), which the embedder has
//! no business tracking, so it belongs here rather than in the host.
//!
//! What is implemented: the legacy encodings, `modifyOtherKeys` (levels 1 and
//! 2 alike), and the parts of the kitty keyboard protocol that change what a
//! key produces, namely disambiguation, event types, and reporting every key
//! as an escape sequence. Alternate keys and associated text (kitty's
//! `REPORT_ALTERNATE_KEYS` and `REPORT_ASSOCIATED_TEXT`) are not encoded; a
//! terminal may set those flags and this encoder simply will not enrich the
//! sequence, which degrades to the disambiguating form rather than misreporting.

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Modifiers: u8 {
        const SHIFT = 1 << 0;
        const CTRL  = 1 << 1;
        const ALT   = 1 << 2;
        const SUPER = 1 << 3;
    }
}

bitflags::bitflags! {
    /// Which of the kitty keyboard flags the terminal currently has set.
    /// Mirrors `rio_vt::ansi::KeyboardModes` without depending on its bit
    /// order, since this crosses a C boundary.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct KittyFlags: u8 {
        const DISAMBIGUATE      = 1 << 0;
        const REPORT_EVENT_TYPES = 1 << 1;
        const REPORT_ALL_AS_ESC = 1 << 2;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyAction {
    #[default]
    Press,
    Repeat,
    Release,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
    F(u8),
    // Modifier keys as keys, for the kitty protocol's report-all mode
    // (its only consumer: legacy encodings have nothing to send for a
    // bare modifier).
    CapsLock,
    ShiftLeft,
    ShiftRight,
    ControlLeft,
    ControlRight,
    AltLeft,
    AltRight,
    SuperLeft,
    SuperRight,
}

impl Key {
    pub fn is_modifier(self) -> bool {
        matches!(
            self,
            Key::CapsLock
                | Key::ShiftLeft
                | Key::ShiftRight
                | Key::ControlLeft
                | Key::ControlRight
                | Key::AltLeft
                | Key::AltRight
                | Key::SuperLeft
                | Key::SuperRight
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KeyEvent {
    pub action: KeyAction,
    /// The key itself, unshifted: `shift+a` is `Char('a')`, and the `A` the
    /// platform produced belongs in `text`.
    pub key: Option<Key>,
    pub mods: Modifiers,
    /// Modifiers the platform already spent producing `text`. On layouts where
    /// a character needs alt (or AltGr), alt must not also be encoded as meta,
    /// or `alt+8` on a Nordic layout sends meta instead of the `[` it printed.
    pub consumed_mods: Modifiers,
    /// What the platform produced for this key, after any dead-key or input
    /// method composition. Empty for keys that produce no text.
    pub text: Option<String>,
    /// True while an input method owns the key. Nothing is encoded: the text
    /// arrives later, when composition commits.
    pub composing: bool,
}

/// Terminal state the encoding depends on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EncodeContext {
    /// DECCKM. Arrows and Home/End switch from `CSI` to `SS3`.
    pub app_cursor: bool,
    pub kitty: KittyFlags,
    /// `CSI > 4 ; n m`. Levels 1 and 2 are treated alike: both ask for
    /// unambiguous reporting of modified keys, and the difference between them
    /// only concerns keys this encoder already reports unambiguously.
    pub modify_other_keys: Option<u8>,
    /// Whether alt should act as meta, prefixing with ESC, instead of letting
    /// the platform's text through. Terminals default this on; on macOS it is
    /// the difference between `alt+d` deleting a word and inserting `∂`.
    pub alt_is_meta: bool,
}

impl Modifiers {
    /// The modifier parameter shared by CSI sequences, kitty included: a
    /// 1-based bitfield where shift is 1, alt 2, ctrl 4 and super 8.
    fn param(self) -> u8 {
        let mut value = 1;
        if self.contains(Modifiers::SHIFT) {
            value += 1;
        }
        if self.contains(Modifiers::ALT) {
            value += 2;
        }
        if self.contains(Modifiers::CTRL) {
            value += 4;
        }
        if self.contains(Modifiers::SUPER) {
            value += 8;
        }
        value
    }
}

impl KeyAction {
    /// Kitty's event type parameter: press is 1 and may be omitted, repeat 2,
    /// release 3.
    fn kitty_param(self) -> u8 {
        match self {
            KeyAction::Press => 1,
            KeyAction::Repeat => 2,
            KeyAction::Release => 3,
        }
    }
}

/// The control byte a key produces with ctrl held, following the ASCII
/// C0 layout that terminals have always used.
fn ctrl_byte(c: char) -> Option<u8> {
    let byte = match c {
        ' ' | '@' => 0x00,
        'a'..='z' => (c as u8) & 0x1f,
        'A'..='Z' => (c as u8) & 0x1f,
        '[' => 0x1b,
        '\\' => 0x1c,
        ']' => 0x1d,
        '^' => 0x1e,
        '_' => 0x1f,
        // ctrl+? is DEL, which is how ctrl+backspace is usually spelled.
        '?' => 0x7f,
        // ctrl+2 and ctrl+3 through ctrl+8 alias the block above on most
        // keyboards, because that is where those symbols sit unshifted.
        '2' => 0x00,
        '3' => 0x1b,
        '4' => 0x1c,
        '5' => 0x1d,
        '6' => 0x1e,
        '7' | '/' => 0x1f,
        '8' => 0x7f,
        _ => return None,
    };
    Some(byte)
}

/// When a key's unmodified form uses SS3 rather than CSI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ss3 {
    /// Only in application cursor mode: the arrows and Home/End follow DECCKM.
    WhenAppCursor,
    /// Always. F1 to F4 are SS3 by convention, independent of DECCKM.
    Always,
}

/// The final byte of a key's legacy CSI form, if it has one, and when SS3
/// applies to it.
fn csi_final(key: Key) -> Option<(char, Ss3)> {
    let pair = match key {
        Key::Up => ('A', Ss3::WhenAppCursor),
        Key::Down => ('B', Ss3::WhenAppCursor),
        Key::Right => ('C', Ss3::WhenAppCursor),
        Key::Left => ('D', Ss3::WhenAppCursor),
        Key::Home => ('H', Ss3::WhenAppCursor),
        Key::End => ('F', Ss3::WhenAppCursor),
        Key::F(1) => ('P', Ss3::Always),
        Key::F(2) => ('Q', Ss3::Always),
        Key::F(3) => ('R', Ss3::Always),
        Key::F(4) => ('S', Ss3::Always),
        _ => return None,
    };
    Some(pair)
}

/// The number in a key's legacy `CSI n ~` form, if it has one.
fn tilde_number(key: Key) -> Option<u8> {
    let number = match key {
        Key::Insert => 2,
        Key::Delete => 3,
        Key::PageUp => 5,
        Key::PageDown => 6,
        Key::F(5) => 15,
        Key::F(6) => 17,
        Key::F(7) => 18,
        Key::F(8) => 19,
        Key::F(9) => 20,
        Key::F(10) => 21,
        Key::F(11) => 23,
        Key::F(12) => 24,
        _ => return None,
    };
    Some(number)
}

/// The codepoint kitty identifies a key by. Text keys are themselves; the rest
/// use the private range the protocol assigns.
fn kitty_codepoint(key: Key) -> u32 {
    match key {
        Key::Char(c) => c as u32,
        Key::Escape => 27,
        Key::Enter => 13,
        Key::Tab => 9,
        Key::Backspace => 127,
        Key::Insert => 2,
        Key::Delete => 3,
        Key::Left => 57417,
        Key::Right => 57418,
        Key::Up => 57419,
        Key::Down => 57420,
        Key::PageUp => 57421,
        Key::PageDown => 57422,
        Key::Home => 57423,
        Key::End => 57424,
        Key::F(n) => 57363 + n as u32 - 1,
        Key::CapsLock => 57358,
        Key::ShiftLeft => 57441,
        Key::ControlLeft => 57442,
        Key::AltLeft => 57443,
        Key::SuperLeft => 57444,
        Key::ShiftRight => 57447,
        Key::ControlRight => 57448,
        Key::AltRight => 57449,
        Key::SuperRight => 57450,
    }
}

fn push_mods_and_event(
    out: &mut String,
    mods: Modifiers,
    action: KeyAction,
    kitty: KittyFlags,
) {
    let report_event =
        kitty.contains(KittyFlags::REPORT_EVENT_TYPES) && action != KeyAction::Press;
    if mods.is_empty() && !report_event {
        return;
    }
    out.push(';');
    out.push_str(&mods.param().to_string());
    if report_event {
        out.push(':');
        out.push_str(&action.kitty_param().to_string());
    }
}

/// Encode in kitty's form: functional keys keep their legacy shape with the
/// modifier parameter, everything else becomes `CSI codepoint ; mods u`.
fn encode_kitty(
    key: Key,
    mods: Modifiers,
    action: KeyAction,
    ctx: &EncodeContext,
) -> Vec<u8> {
    let mut out = String::from("\x1b[");
    if let Some(number) = tilde_number(key) {
        out.push_str(&number.to_string());
        push_mods_and_event(&mut out, mods, action, ctx.kitty);
        out.push('~');
        return out.into_bytes();
    }

    if let Some((final_byte, _)) = csi_final(key) {
        // A modifier or event type needs the leading 1 so the parameters line
        // up: `CSI 1 ; 5 A` rather than `CSI ; 5 A`.
        let has_params = !mods.is_empty()
            || (ctx.kitty.contains(KittyFlags::REPORT_EVENT_TYPES)
                && action != KeyAction::Press);
        if has_params {
            out.push('1');
        }
        push_mods_and_event(&mut out, mods, action, ctx.kitty);
        out.push(final_byte);
        return out.into_bytes();
    }

    out.push_str(&kitty_codepoint(key).to_string());
    push_mods_and_event(&mut out, mods, action, ctx.kitty);
    out.push('u');
    out.into_bytes()
}

/// `CSI 27 ; mods ; codepoint ~`, xterm's unambiguous form for a modified key.
fn encode_modify_other_keys(codepoint: u32, mods: Modifiers) -> Vec<u8> {
    format!("\x1b[27;{};{}~", mods.param(), codepoint).into_bytes()
}

/// The legacy encoding: what terminals send with no keyboard protocol active.
fn encode_legacy(
    key: Key,
    mods: Modifiers,
    text: Option<&str>,
    ctx: &EncodeContext,
) -> Option<Vec<u8>> {
    // Any modifier at all takes the parameterised CSI form for keys that have
    // one, which is also what unmodified keys use outside application mode.
    if let Some((final_byte, ss3)) = csi_final(key) {
        let bytes = if mods.is_empty() {
            if ss3 == Ss3::Always || ctx.app_cursor {
                format!("\x1bO{final_byte}")
            } else {
                format!("\x1b[{final_byte}")
            }
        } else {
            format!("\x1b[1;{}{final_byte}", mods.param())
        };
        return Some(bytes.into_bytes());
    }

    if let Some(number) = tilde_number(key) {
        let bytes = if mods.is_empty() {
            format!("\x1b[{number}~")
        } else {
            format!("\x1b[{number};{}~", mods.param())
        };
        return Some(bytes.into_bytes());
    }

    let alt = mods.contains(Modifiers::ALT);
    let ctrl = mods.contains(Modifiers::CTRL);

    let base: Vec<u8> = match key {
        Key::Enter => vec![b'\r'],
        Key::Tab => {
            if mods.contains(Modifiers::SHIFT) {
                return Some(b"\x1b[Z".to_vec());
            }
            vec![b'\t']
        }
        Key::Backspace => vec![0x7f],
        Key::Escape => vec![0x1b],
        Key::Char(c) => {
            if ctrl {
                // ctrl+key that has no control byte sends nothing legacy can
                // express; `modifyOtherKeys` or kitty mode is how a program
                // asks to see it.
                vec![ctrl_byte(c)?]
            } else if let Some(text) = text.filter(|t| !t.is_empty()) {
                // Prefer what the platform produced: it already accounts for
                // shift, the layout, and any dead key.
                text.as_bytes().to_vec()
            } else {
                c.to_string().into_bytes()
            }
        }
        // Only kitty report-all mode encodes bare modifiers; encode()
        // returns before reaching the legacy form.
        Key::CapsLock
        | Key::ShiftLeft
        | Key::ShiftRight
        | Key::ControlLeft
        | Key::ControlRight
        | Key::AltLeft
        | Key::AltRight
        | Key::SuperLeft
        | Key::SuperRight => return None,
        // Handled above.
        Key::Up
        | Key::Down
        | Key::Left
        | Key::Right
        | Key::Home
        | Key::End
        | Key::Insert
        | Key::Delete
        | Key::PageUp
        | Key::PageDown
        | Key::F(_) => return None,
    };

    if alt && ctx.alt_is_meta {
        let mut bytes = Vec::with_capacity(base.len() + 1);
        bytes.push(0x1b);
        bytes.extend_from_slice(&base);
        return Some(bytes);
    }

    Some(base)
}

/// Whether the legacy form of this key loses information the terminal asked to
/// see: ctrl or alt combined with a key whose control byte is ambiguous or
/// absent, and Escape, which is indistinguishable from the start of any
/// sequence.
fn needs_disambiguation(key: Key, mods: Modifiers) -> bool {
    if mods.contains(Modifiers::CTRL) || mods.contains(Modifiers::ALT) {
        return true;
    }
    matches!(key, Key::Escape)
}

pub fn encode(event: &KeyEvent, ctx: &EncodeContext) -> Option<Vec<u8>> {
    // An input method owns the key. Its text arrives on commit.
    if event.composing {
        return None;
    }

    // No key: a text-only event, such as an input method committing a
    // composition. The composed text goes to the program as-is; there is
    // no keystroke to encode.
    let Some(key) = event.key else {
        if event.action != KeyAction::Press {
            return None;
        }
        return event
            .text
            .clone()
            .filter(|text| !text.is_empty())
            .map(String::into_bytes);
    };

    // Releases exist only for programs that asked for event types.
    if event.action == KeyAction::Release
        && !ctx.kitty.contains(KittyFlags::REPORT_EVENT_TYPES)
    {
        return None;
    }

    // Bare modifiers exist only in kitty's report-all mode; every other
    // encoding has nothing to send for one.
    if key.is_modifier() {
        if !ctx.kitty.contains(KittyFlags::REPORT_ALL_AS_ESC) {
            return None;
        }
        let mods = event.mods.difference(event.consumed_mods);
        return Some(encode_kitty(key, mods, event.action, ctx));
    }

    // A modifier the platform spent on producing the text is not also a
    // modifier the program should see.
    let mods = event.mods.difference(event.consumed_mods);
    let text = event.text.as_deref();

    let kitty_active = !ctx.kitty.is_empty();
    if kitty_active {
        let all = ctx.kitty.contains(KittyFlags::REPORT_ALL_AS_ESC);
        let event_types = ctx.kitty.contains(KittyFlags::REPORT_EVENT_TYPES)
            && event.action != KeyAction::Press;
        let disambiguate = ctx.kitty.contains(KittyFlags::DISAMBIGUATE)
            && needs_disambiguation(key, mods);

        if all || event_types || disambiguate {
            return Some(encode_kitty(key, mods, event.action, ctx));
        }
    }

    // Text keys with a modifier the legacy form mangles, when the program has
    // asked for the unambiguous form.
    if ctx.modify_other_keys.is_some_and(|level| level >= 1) {
        if let Key::Char(c) = key {
            if mods.contains(Modifiers::CTRL) || mods.contains(Modifiers::ALT) {
                return Some(encode_modify_other_keys(c as u32, mods));
            }
        }
    }

    // Super is the platform's own modifier: a terminal has nothing to send for
    // it, and swallowing it here keeps `cmd+c` from reaching the shell as `c`.
    if mods.contains(Modifiers::SUPER) {
        return None;
    }

    encode_legacy(key, mods, text, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(key: Key) -> KeyEvent {
        KeyEvent {
            key: Some(key),
            ..Default::default()
        }
    }

    fn with_mods(key: Key, mods: Modifiers) -> KeyEvent {
        KeyEvent {
            key: Some(key),
            mods,
            ..Default::default()
        }
    }

    fn legacy() -> EncodeContext {
        EncodeContext {
            alt_is_meta: true,
            ..Default::default()
        }
    }

    fn kitty(flags: KittyFlags) -> EncodeContext {
        EncodeContext {
            kitty: flags,
            alt_is_meta: true,
            ..Default::default()
        }
    }

    #[test]
    fn plain_char_uses_platform_text() {
        // The platform's text wins, which is what makes shift, dead keys and
        // non-Latin layouts work without this encoder knowing about them.
        let event = KeyEvent {
            key: Some(Key::Char('a')),
            mods: Modifiers::SHIFT,
            text: Some("A".into()),
            ..Default::default()
        };
        assert_eq!(encode(&event, &legacy()), Some(b"A".to_vec()));
    }

    #[test]
    fn ctrl_covers_more_than_letters() {
        // The gap that made ctrl+[ and ctrl+space do nothing at all.
        for (c, expected) in [
            ('c', 0x03),
            ('C', 0x03),
            (' ', 0x00),
            ('[', 0x1b),
            ('\\', 0x1c),
            (']', 0x1d),
            ('_', 0x1f),
            ('/', 0x1f),
            ('?', 0x7f),
        ] {
            let event = with_mods(Key::Char(c), Modifiers::CTRL);
            assert_eq!(
                encode(&event, &legacy()),
                Some(vec![expected]),
                "ctrl+{c:?}"
            );
        }
    }

    #[test]
    fn alt_is_meta_when_enabled() {
        let event = with_mods(Key::Char('d'), Modifiers::ALT);
        assert_eq!(encode(&event, &legacy()), Some(vec![0x1b, b'd']));

        // With the policy off, the platform's text goes through instead, which
        // on macOS is how you would type ∂.
        let ctx = EncodeContext::default();
        let event = KeyEvent {
            key: Some(Key::Char('d')),
            mods: Modifiers::ALT,
            text: Some("∂".into()),
            ..Default::default()
        };
        assert_eq!(encode(&event, &ctx), Some("∂".as_bytes().to_vec()));
    }

    #[test]
    fn consumed_alt_is_not_meta() {
        // A layout where alt is part of typing the character: encoding meta
        // here would send ESC instead of the bracket the user typed.
        let event = KeyEvent {
            key: Some(Key::Char('8')),
            mods: Modifiers::ALT,
            consumed_mods: Modifiers::ALT,
            text: Some("[".into()),
            ..Default::default()
        };
        assert_eq!(encode(&event, &legacy()), Some(b"[".to_vec()));
    }

    #[test]
    fn arrows_follow_application_cursor_mode() {
        let ctx = EncodeContext {
            app_cursor: true,
            ..legacy()
        };
        assert_eq!(encode(&press(Key::Up), &legacy()), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode(&press(Key::Up), &ctx), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn modified_arrows_carry_the_modifier() {
        // Each of these silently did nothing before, because the host could
        // not express them.
        let cases = [
            (Modifiers::SHIFT, "\x1b[1;2D"),
            (Modifiers::ALT, "\x1b[1;3D"),
            (Modifiers::CTRL, "\x1b[1;5D"),
            (Modifiers::CTRL | Modifiers::SHIFT, "\x1b[1;6D"),
        ];
        for (mods, expected) in cases {
            let event = with_mods(Key::Left, mods);
            assert_eq!(
                encode(&event, &legacy()),
                Some(expected.as_bytes().to_vec()),
                "{mods:?}"
            );
        }
    }

    // Modifier keys are keys only to kitty's report-all mode: a press
    // encodes with the protocol's assigned number, and nothing at all is
    // sent in legacy mode or with lesser kitty flags.
    #[test]
    fn modifier_keys_report_only_in_kitty_report_all() {
        let press = KeyEvent {
            key: Some(Key::ShiftLeft),
            mods: Modifiers::SHIFT,
            ..Default::default()
        };
        assert_eq!(encode(&press, &EncodeContext::default()), None);

        let ctx = EncodeContext {
            kitty: KittyFlags::DISAMBIGUATE
                | KittyFlags::REPORT_EVENT_TYPES
                | KittyFlags::REPORT_ALL_AS_ESC,
            ..Default::default()
        };
        assert_eq!(encode(&press, &ctx), Some(b"\x1b[57441;2u".to_vec()));
        let release = KeyEvent {
            key: Some(Key::ShiftLeft),
            action: KeyAction::Release,
            ..Default::default()
        };
        assert_eq!(encode(&release, &ctx), Some(b"\x1b[57441;1:3u".to_vec()));
    }

    // RIO_KEY_NONE: an input method commit carries text and no key.
    #[test]
    fn text_only_events_pass_text_through() {
        let event = KeyEvent {
            key: None,
            text: Some("\u{3053}\u{3093}".into()),
            ..Default::default()
        };
        assert_eq!(
            encode(&event, &EncodeContext::default()),
            Some("\u{3053}\u{3093}".as_bytes().to_vec())
        );
    }

    #[test]
    fn composing_encodes_nothing() {
        let event = KeyEvent {
            key: Some(Key::Char('a')),
            text: Some("a".into()),
            composing: true,
            ..Default::default()
        };
        assert_eq!(encode(&event, &legacy()), None);
    }

    #[test]
    fn release_only_when_event_types_are_requested() {
        let event = KeyEvent {
            key: Some(Key::Char('a')),
            action: KeyAction::Release,
            ..Default::default()
        };
        assert_eq!(encode(&event, &legacy()), None);

        let ctx = kitty(KittyFlags::REPORT_EVENT_TYPES);
        assert_eq!(encode(&event, &ctx), Some(b"\x1b[97;1:3u".to_vec()));
    }

    #[test]
    fn kitty_disambiguates_only_what_legacy_loses() {
        let ctx = kitty(KittyFlags::DISAMBIGUATE);

        // Escape and ctrl combos become CSI u.
        assert_eq!(
            encode(&press(Key::Escape), &ctx),
            Some(b"\x1b[27u".to_vec())
        );
        let ctrl_i = with_mods(Key::Char('i'), Modifiers::CTRL);
        assert_eq!(encode(&ctrl_i, &ctx), Some(b"\x1b[105;5u".to_vec()));

        // A plain text key still sends its text: disambiguation alone does not
        // ask for everything as an escape sequence.
        let event = KeyEvent {
            key: Some(Key::Char('a')),
            text: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(encode(&event, &ctx), Some(b"a".to_vec()));
    }

    #[test]
    fn kitty_reports_every_key_when_asked() {
        let ctx = kitty(KittyFlags::REPORT_ALL_AS_ESC);
        let event = KeyEvent {
            key: Some(Key::Char('a')),
            text: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(encode(&event, &ctx), Some(b"\x1b[97u".to_vec()));
        assert_eq!(
            encode(&press(Key::Left), &ctx),
            Some(b"\x1b[D".to_vec()),
            "functional keys keep their legacy shape"
        );
    }

    #[test]
    fn kitty_repeat_carries_the_event_type() {
        let ctx = kitty(KittyFlags::REPORT_EVENT_TYPES);
        let event = KeyEvent {
            key: Some(Key::Left),
            action: KeyAction::Repeat,
            ..Default::default()
        };
        assert_eq!(encode(&event, &ctx), Some(b"\x1b[1;1:2D".to_vec()));
    }

    #[test]
    fn modify_other_keys_reports_ctrl_combinations() {
        let ctx = EncodeContext {
            modify_other_keys: Some(2),
            ..legacy()
        };
        let event = with_mods(Key::Char('i'), Modifiers::CTRL);
        assert_eq!(encode(&event, &ctx), Some(b"\x1b[27;5;105~".to_vec()));

        // Unmodified keys are untouched by it.
        let plain = KeyEvent {
            key: Some(Key::Char('a')),
            text: Some("a".into()),
            ..Default::default()
        };
        assert_eq!(encode(&plain, &ctx), Some(b"a".to_vec()));
    }

    #[test]
    fn super_is_not_the_terminals_to_send() {
        let event = with_mods(Key::Char('c'), Modifiers::SUPER);
        assert_eq!(encode(&event, &legacy()), None);
    }

    #[test]
    fn shift_tab_is_back_tab() {
        let event = with_mods(Key::Tab, Modifiers::SHIFT);
        assert_eq!(encode(&event, &legacy()), Some(b"\x1b[Z".to_vec()));
    }

    #[test]
    fn function_keys_split_between_ss3_and_tilde() {
        assert_eq!(
            encode(&press(Key::F(1)), &legacy()),
            Some(b"\x1bOP".to_vec())
        );
        assert_eq!(
            encode(&press(Key::F(5)), &legacy()),
            Some(b"\x1b[15~".to_vec())
        );
        let shifted = with_mods(Key::F(1), Modifiers::SHIFT);
        assert_eq!(encode(&shifted, &legacy()), Some(b"\x1b[1;2P".to_vec()));
    }
}
