//! Key event encoding.
//!
//! An embedder hands over the platform's key event more or less as it arrived,
//! including the text the platform produced for it, and this module decides
//! what reaches the pty. The decision needs terminal state (application cursor
//! mode, the kitty keyboard flags, `modifyOtherKeys`), which the embedder has
//! no business tracking, so it belongs here rather than in the host.
//!
//! What is implemented: the legacy encodings, `modifyOtherKeys` (levels 1 and
//! 2 alike), and the kitty keyboard protocol: disambiguation, event types,
//! reporting every key as an escape sequence, the shifted alternate key, and
//! associated text. The base layout key (the third alternate) is not reported:
//! the embedder hands over the key it saw, not the US-layout key in its place.

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
        /// `CSI key:shifted u`: the key shift turned it into, so a program
        /// matching on `G` sees `G` and not only `g` plus shift.
        const REPORT_ALTERNATE_KEYS = 1 << 3;
        /// `CSI key ; mods ; text u`: the text the key produced. Only
        /// meaningful with `REPORT_ALL_AS_ESC`, where text keys stop
        /// arriving as plain text.
        const REPORT_ASSOCIATED_TEXT = 1 << 4;
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
    shifted: Option<u32>,
    text: Option<&str>,
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
    if let Some(shifted) = shifted {
        out.push(':');
        out.push_str(&shifted.to_string());
    }
    let associated = associated_text(key, mods, action, text, ctx);
    if associated.is_some() && mods.is_empty() && !has_event_param(action, ctx.kitty) {
        // Text is the third parameter, so the modifiers slot stays, empty,
        // as kitty and ghostty write it: `CSI 97 ; ; 97 u`.
        out.push(';');
    } else {
        push_mods_and_event(&mut out, mods, action, ctx.kitty);
    }
    if let Some(codepoints) = associated {
        for (index, codepoint) in codepoints.iter().enumerate() {
            out.push(if index == 0 { ';' } else { ':' });
            out.push_str(&codepoint.to_string());
        }
    }
    out.push('u');
    out.into_bytes()
}

/// Whether `push_mods_and_event` writes an event type for this action.
fn has_event_param(action: KeyAction, kitty: KittyFlags) -> bool {
    kitty.contains(KittyFlags::REPORT_EVENT_TYPES) && action != KeyAction::Press
}

/// The shifted key, for `REPORT_ALTERNATE_KEYS`: the single character shift
/// produced when it differs from the key itself (`g` gives `G`, `1` gives `!`
/// on a US layout).
///
/// `pressed` is every modifier held, before removing the ones the platform
/// spent on the text: with alt or ctrl also down, the text is no longer what
/// shift alone makes (alt+shift+g is `˝` on a Mac), so it isn't the shifted
/// key and nothing is reported.
fn alternate_key(
    key: Key,
    pressed: Modifiers,
    text: Option<&str>,
    kitty: KittyFlags,
) -> Option<u32> {
    if !kitty.contains(KittyFlags::REPORT_ALTERNATE_KEYS)
        || !pressed.contains(Modifiers::SHIFT)
        || pressed.intersects(Modifiers::ALT | Modifiers::CTRL)
    {
        return None;
    }
    let Key::Char(base) = key else { return None };
    let mut chars = text?.chars();
    let shifted = chars.next()?;
    if chars.next().is_some() || shifted == base || shifted.is_control() {
        return None;
    }
    Some(shifted as u32)
}

/// The text to report, for `REPORT_ASSOCIATED_TEXT`: what a press or repeat
/// of a text key typed, never control characters and never on release.
///
/// A chord types nothing: embedders translate text with ctrl (and alt, when
/// it acts as meta) taken out, so ctrl+a arrives with text `a`. Reporting it
/// would tell the program that ctrl+a typed an `a`. Alt that is not meta is
/// the Mac's Option, which types (option+g is `©`), so it keeps its text;
/// ghostty draws the same line.
///
/// Control characters are skipped one by one, so what's left is only what
/// can be typed.
fn associated_text(
    key: Key,
    mods: Modifiers,
    action: KeyAction,
    text: Option<&str>,
    ctx: &EncodeContext,
) -> Option<Vec<u32>> {
    let chord = mods.intersects(Modifiers::CTRL | Modifiers::SUPER)
        || (mods.contains(Modifiers::ALT) && ctx.alt_is_meta);
    if !ctx.kitty.contains(KittyFlags::REPORT_ASSOCIATED_TEXT)
        || !ctx.kitty.contains(KittyFlags::REPORT_ALL_AS_ESC)
        || action == KeyAction::Release
        || chord
        || !matches!(key, Key::Char(_))
    {
        return None;
    }
    let codepoints: Vec<u32> = text?
        .chars()
        .filter(|c| !c.is_control())
        .map(u32::from)
        .collect();
    (!codepoints.is_empty()).then_some(codepoints)
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
        return Some(encode_kitty(key, event.mods, event.action, None, None, ctx));
    }

    // A modifier the platform spent on producing the text is not also a
    // modifier the program should see as a chord: it decides between text
    // and an escape, and what legacy sends. With no text nothing was spent,
    // so every modifier stays (alt+backspace is still alt). Kitty sequences
    // still report every modifier held, as kitty and ghostty do.
    let text = event.text.as_deref();
    let mods = if text.is_some_and(|text| !text.is_empty()) {
        event.mods.difference(event.consumed_mods)
    } else {
        event.mods
    };

    let kitty_active = !ctx.kitty.is_empty();
    if kitty_active {
        let all = ctx.kitty.contains(KittyFlags::REPORT_ALL_AS_ESC);
        let disambiguate = ctx.kitty.contains(KittyFlags::DISAMBIGUATE)
            && needs_disambiguation(key, mods);

        // Below report-all, Enter, Tab and Backspace keep their legacy bytes
        // so a shell stays usable after a program that set the mode crashes,
        // and the spec gives them no release events either.
        let legacy_editing_key = matches!(key, Key::Enter | Key::Tab | Key::Backspace);
        if !all && legacy_editing_key && event.action == KeyAction::Release {
            return None;
        }

        // A key that goes out as plain text on press does so on repeat too:
        // holding `g` types `ggg`, not escape codes. Only releases, and keys
        // that are escape codes anyway, carry the event type.
        let typed_text = matches!(key, Key::Char(_))
            && text.is_some_and(|text| {
                !text.is_empty() && !text.chars().any(char::is_control)
            })
            && !mods.contains(Modifiers::SUPER);
        let repeats_as_press =
            !all && !disambiguate && (typed_text || legacy_editing_key);
        let event_types = ctx.kitty.contains(KittyFlags::REPORT_EVENT_TYPES)
            && match event.action {
                KeyAction::Press => false,
                KeyAction::Repeat => !repeats_as_press,
                KeyAction::Release => true,
            };

        if all || event_types || disambiguate {
            let shifted = alternate_key(key, event.mods, text, ctx.kitty);
            return Some(encode_kitty(
                key,
                event.mods,
                event.action,
                shifted,
                text,
                ctx,
            ));
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

    fn typed(c: char, mods: Modifiers, text: &str) -> KeyEvent {
        KeyEvent {
            key: Some(Key::Char(c)),
            mods,
            text: Some(text.to_string()),
            ..Default::default()
        }
    }

    /// What yazi pushes: disambiguate, alternate keys, report all as escape
    /// codes and associated text (kitty flags 1 | 4 | 8 | 16).
    fn yazi() -> EncodeContext {
        kitty(
            KittyFlags::DISAMBIGUATE
                | KittyFlags::REPORT_ALL_AS_ESC
                | KittyFlags::REPORT_ALTERNATE_KEYS
                | KittyFlags::REPORT_ASSOCIATED_TEXT,
        )
    }

    #[test]
    fn alternate_key_reports_the_shifted_character() {
        // The kitty spec's own example: shift+a is `CSI 97:65 ; 2 u`.
        let ctx =
            kitty(KittyFlags::REPORT_ALL_AS_ESC | KittyFlags::REPORT_ALTERNATE_KEYS);
        let event = typed('a', Modifiers::SHIFT, "A");
        assert_eq!(encode(&event, &ctx), Some(b"\x1b[97:65;2u".to_vec()));
        // A symbol shift produces on the same key: `1` gives `!`.
        let bang = typed('1', Modifiers::SHIFT, "!");
        assert_eq!(encode(&bang, &ctx), Some(b"\x1b[49:33;2u".to_vec()));
    }

    #[test]
    fn alternate_key_needs_shift_and_a_different_character() {
        let ctx =
            kitty(KittyFlags::REPORT_ALL_AS_ESC | KittyFlags::REPORT_ALTERNATE_KEYS);
        assert_eq!(
            encode(&typed('a', Modifiers::empty(), "a"), &ctx),
            Some(b"\x1b[97u".to_vec())
        );
        // ctrl+shift+a produces no text, so there is no shifted key to add.
        let mut ctrl = typed('a', Modifiers::SHIFT | Modifiers::CTRL, "");
        ctrl.text = None;
        assert_eq!(encode(&ctrl, &ctx), Some(b"\x1b[97;6u".to_vec()));
    }

    #[test]
    fn alternate_key_is_only_what_shift_alone_makes() {
        let ctx =
            kitty(KittyFlags::REPORT_ALL_AS_ESC | KittyFlags::REPORT_ALTERNATE_KEYS);
        // alt+shift+g types `˝` on a Mac: text, but not g's shifted key.
        let alt = typed('g', Modifiers::SHIFT | Modifiers::ALT, "\u{2dd}");
        assert_eq!(encode(&alt, &ctx), Some(b"\x1b[103;4u".to_vec()));
        // Same when the platform marks alt as spent on the text.
        let mut consumed = alt.clone();
        consumed.consumed_mods = Modifiers::ALT;
        assert_eq!(encode(&consumed, &ctx), Some(b"\x1b[103;4u".to_vec()));
    }

    #[test]
    fn yazi_sees_shift_g_as_capital_g() {
        // Issue #2119: shift was lost because only `CSI 103 ; 2 u` went out.
        let event = typed('g', Modifiers::SHIFT, "G");
        assert_eq!(encode(&event, &yazi()), Some(b"\x1b[103:71;2;71u".to_vec()));
    }

    #[test]
    fn associated_text_keeps_the_modifier_slot() {
        let event = typed('g', Modifiers::empty(), "g");
        assert_eq!(encode(&event, &yazi()), Some(b"\x1b[103;;103u".to_vec()));
    }

    #[test]
    fn associated_text_is_never_sent_on_release_or_for_control() {
        let mut release = typed('g', Modifiers::empty(), "g");
        release.action = KeyAction::Release;
        let ctx = kitty(
            KittyFlags::REPORT_ALL_AS_ESC
                | KittyFlags::REPORT_EVENT_TYPES
                | KittyFlags::REPORT_ASSOCIATED_TEXT,
        );
        assert_eq!(encode(&release, &ctx), Some(b"\x1b[103;1:3u".to_vec()));
        let control = typed('a', Modifiers::CTRL, "\u{1}");
        assert_eq!(encode(&control, &yazi()), Some(b"\x1b[97;5u".to_vec()));
    }

    #[test]
    fn chords_carry_no_text() {
        // Embedders translate without ctrl, so ctrl+a comes with text `a`.
        let ctrl = typed('a', Modifiers::CTRL, "a");
        assert_eq!(encode(&ctrl, &yazi()), Some(b"\x1b[97;5u".to_vec()));
        // Alt acting as meta: the `g` is not typed text.
        let meta = typed('g', Modifiers::ALT, "g");
        assert_eq!(encode(&meta, &yazi()), Some(b"\x1b[103;3u".to_vec()));
        // Alt as a Mac's Option, spent on the text: the `©` was typed, and
        // alt is still reported as held, as kitty and ghostty do.
        let mut option = typed('g', Modifiers::ALT, "\u{a9}");
        option.consumed_mods = Modifiers::ALT;
        let mac = EncodeContext {
            alt_is_meta: false,
            ..yazi()
        };
        assert_eq!(encode(&option, &mac), Some(b"\x1b[103;3;169u".to_vec()));
    }

    #[test]
    fn option_typed_text_is_text_outside_report_all() {
        // Disambiguate alone: the `©` option typed goes through as text,
        // not as an alt+g chord.
        let mut option = typed('g', Modifiers::ALT, "\u{a9}");
        option.consumed_mods = Modifiers::ALT;
        let ctx = EncodeContext {
            alt_is_meta: false,
            ..kitty(KittyFlags::DISAMBIGUATE)
        };
        assert_eq!(encode(&option, &ctx), Some("\u{a9}".as_bytes().to_vec()));
    }

    #[test]
    fn consumed_alt_without_text_stays_a_chord() {
        // Nothing was typed, so nothing was spent: alt+backspace is alt.
        let event = KeyEvent {
            key: Some(Key::Backspace),
            mods: Modifiers::ALT,
            consumed_mods: Modifiers::ALT,
            ..Default::default()
        };
        assert_eq!(
            encode(&event, &kitty(KittyFlags::DISAMBIGUATE)),
            Some(b"\x1b[127;3u".to_vec())
        );
    }

    #[test]
    fn associated_text_skips_control_characters() {
        let event = typed('a', Modifiers::SHIFT, "A\n");
        assert_eq!(encode(&event, &yazi()), Some(b"\x1b[97;2;65u".to_vec()));
    }

    #[test]
    fn release_names_the_same_shifted_key_as_the_press() {
        let ctx = kitty(
            KittyFlags::REPORT_ALL_AS_ESC
                | KittyFlags::REPORT_EVENT_TYPES
                | KittyFlags::REPORT_ALTERNATE_KEYS
                | KittyFlags::REPORT_ASSOCIATED_TEXT,
        );
        let mut release = typed('g', Modifiers::SHIFT, "G");
        release.action = KeyAction::Release;
        assert_eq!(encode(&release, &ctx), Some(b"\x1b[103:71;2:3u".to_vec()));
    }

    #[test]
    fn repeat_keeps_the_shifted_key_and_text() {
        let ctx = EncodeContext {
            kitty: yazi().kitty | KittyFlags::REPORT_EVENT_TYPES,
            ..yazi()
        };
        let mut repeat = typed('g', Modifiers::SHIFT, "G");
        repeat.action = KeyAction::Repeat;
        assert_eq!(encode(&repeat, &ctx), Some(b"\x1b[103:71;2:2;71u".to_vec()));
        // A plain repeat writes the event type, so the text follows it
        // directly rather than after an empty slot.
        let mut plain = typed('g', Modifiers::empty(), "g");
        plain.action = KeyAction::Repeat;
        assert_eq!(encode(&plain, &ctx), Some(b"\x1b[103;1:2;103u".to_vec()));
    }

    #[test]
    fn caps_lock_capital_is_text_not_a_shifted_key() {
        // Caps lock types `G` with no shift held: there is no shifted key,
        // and the text says what was typed.
        let event = typed('g', Modifiers::empty(), "G");
        assert_eq!(encode(&event, &yazi()), Some(b"\x1b[103;;71u".to_vec()));
    }

    #[test]
    fn shifted_key_on_a_non_latin_layout() {
        // Russian layout: shift+п is П.
        let event = typed('\u{43f}', Modifiers::SHIFT, "\u{41f}");
        assert_eq!(
            encode(&event, &yazi()),
            Some(b"\x1b[1087:1055;2;1055u".to_vec())
        );
    }

    #[test]
    fn text_of_several_codepoints_is_colon_separated() {
        // An `e` followed by a combining acute, as some layouts and dead
        // keys produce. Not one character, so no shifted key either.
        let event = typed('e', Modifiers::SHIFT, "E\u{301}");
        assert_eq!(
            encode(&event, &yazi()),
            Some(b"\x1b[101;2;69:769u".to_vec())
        );
    }

    #[test]
    fn shift_space_has_no_shifted_key() {
        // Shift doesn't change a space, so only the text is added.
        let event = typed(' ', Modifiers::SHIFT, " ");
        assert_eq!(encode(&event, &yazi()), Some(b"\x1b[32;2;32u".to_vec()));
    }

    #[test]
    fn super_chord_carries_no_text() {
        let event = typed('g', Modifiers::SUPER, "g");
        assert_eq!(encode(&event, &yazi()), Some(b"\x1b[103;9u".to_vec()));
    }

    #[test]
    fn alternate_keys_alone_leave_shifted_text_as_text() {
        // Without report-all, shift+g is still plain text: the alternate
        // only enriches keys that are sent as escape codes.
        let ctx = kitty(KittyFlags::DISAMBIGUATE | KittyFlags::REPORT_ALTERNATE_KEYS);
        let event = typed('g', Modifiers::SHIFT, "G");
        assert_eq!(encode(&event, &ctx), Some(b"G".to_vec()));
        // A chord that is an escape gets no alternate when ctrl is held.
        let mut ctrl = typed('g', Modifiers::SHIFT | Modifiers::CTRL, "G");
        ctrl.text = None;
        assert_eq!(encode(&ctrl, &ctx), Some(b"\x1b[103;6u".to_vec()));
    }

    #[test]
    fn bare_modifiers_report_every_modifier_held() {
        let ctx = kitty(KittyFlags::REPORT_ALL_AS_ESC);
        let event = KeyEvent {
            key: Some(Key::ShiftLeft),
            mods: Modifiers::SHIFT | Modifiers::ALT,
            consumed_mods: Modifiers::ALT,
            ..Default::default()
        };
        assert_eq!(encode(&event, &ctx), Some(b"\x1b[57441;4u".to_vec()));
    }

    #[test]
    fn legacy_consumed_alt_without_text_is_still_meta() {
        // Nothing typed, nothing spent: alt+backspace stays meta+DEL.
        let event = KeyEvent {
            key: Some(Key::Backspace),
            mods: Modifiers::ALT,
            consumed_mods: Modifiers::ALT,
            ..Default::default()
        };
        assert_eq!(encode(&event, &legacy()), Some(b"\x1b\x7f".to_vec()));
    }

    fn event_types() -> EncodeContext {
        kitty(KittyFlags::DISAMBIGUATE | KittyFlags::REPORT_EVENT_TYPES)
    }

    fn repeated(mut event: KeyEvent) -> KeyEvent {
        event.action = KeyAction::Repeat;
        event
    }

    #[test]
    fn held_text_keys_repeat_as_text() {
        // Holding `g` in an editor types `ggg`, as kitty and ghostty send it.
        let plain = repeated(typed('g', Modifiers::empty(), "g"));
        assert_eq!(encode(&plain, &event_types()), Some(b"g".to_vec()));
        let shifted = repeated(typed('g', Modifiers::SHIFT, "G"));
        assert_eq!(encode(&shifted, &event_types()), Some(b"G".to_vec()));
    }

    #[test]
    fn held_chords_and_named_keys_repeat_as_escapes() {
        // ctrl+g is an escape on press, so its repeat carries the event type.
        let mut ctrl = repeated(typed('g', Modifiers::CTRL, "g"));
        ctrl.text = Some("g".into());
        assert_eq!(
            encode(&ctrl, &event_types()),
            Some(b"\x1b[103;5:2u".to_vec())
        );
        // Arrows are escape codes either way, so the repeat says so.
        let up = repeated(press(Key::Up));
        assert_eq!(encode(&up, &event_types()), Some(b"\x1b[1;1:2A".to_vec()));
    }

    #[test]
    fn held_enter_tab_and_backspace_keep_legacy_bytes() {
        let ctx = event_types();
        assert_eq!(
            encode(&repeated(press(Key::Enter)), &ctx),
            Some(b"\r".to_vec())
        );
        assert_eq!(
            encode(&repeated(press(Key::Tab)), &ctx),
            Some(b"\t".to_vec())
        );
        assert_eq!(
            encode(&repeated(press(Key::Backspace)), &ctx),
            Some(vec![0x7f])
        );
    }

    #[test]
    fn enter_tab_and_backspace_release_only_with_report_all() {
        for key in [Key::Enter, Key::Tab, Key::Backspace] {
            let mut release = press(key);
            release.action = KeyAction::Release;
            assert_eq!(encode(&release, &event_types()), None, "{key:?}");
        }
        let all = kitty(KittyFlags::REPORT_EVENT_TYPES | KittyFlags::REPORT_ALL_AS_ESC);
        let mut enter = press(Key::Enter);
        enter.action = KeyAction::Release;
        assert_eq!(encode(&enter, &all), Some(b"\x1b[13;1:3u".to_vec()));
    }

    #[test]
    fn text_keys_still_release_as_escapes() {
        // Releases never insert text: they are always the escape form.
        let mut release = typed('g', Modifiers::empty(), "g");
        release.action = KeyAction::Release;
        assert_eq!(
            encode(&release, &event_types()),
            Some(b"\x1b[103;1:3u".to_vec())
        );
    }

    #[test]
    fn associated_text_needs_report_all() {
        // Without report-all, text keys arrive as plain text anyway.
        let ctx = kitty(KittyFlags::DISAMBIGUATE | KittyFlags::REPORT_ASSOCIATED_TEXT);
        let event = typed('g', Modifiers::SHIFT, "G");
        assert_eq!(encode(&event, &ctx), Some(b"G".to_vec()));
    }
}
