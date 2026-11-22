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

use std::fmt::Write as _;

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
    /// A numpad key, apart from the main keys with the same symbols: in
    /// application keypad mode (DECKPAM) and in kitty mode it has codes of
    /// its own.
    Keypad(Keypad),
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

/// The numpad keys a terminal encodes apart from the main keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keypad {
    /// 0 to 9.
    Digit(u8),
    Decimal,
    Divide,
    Multiply,
    Subtract,
    Add,
    Enter,
    Equal,
}

impl Keypad {
    /// The key from the symbol on it, as the C API carries it: `'0'` to
    /// `'9'`, `.`, `/`, `*`, `-`, `+`, `=`, and `'\r'` for Enter.
    pub fn from_symbol(symbol: char) -> Option<Self> {
        let key = match symbol {
            '0'..='9' => Keypad::Digit(symbol as u8 - b'0'),
            '.' => Keypad::Decimal,
            '/' => Keypad::Divide,
            '*' => Keypad::Multiply,
            '-' => Keypad::Subtract,
            '+' => Keypad::Add,
            '=' => Keypad::Equal,
            '\r' => Keypad::Enter,
            _ => return None,
        };
        Some(key)
    }

    /// What the key types outside application keypad mode.
    fn symbol(self) -> char {
        match self {
            Keypad::Digit(digit) => char::from(b'0' + digit.min(9)),
            Keypad::Decimal => '.',
            Keypad::Divide => '/',
            Keypad::Multiply => '*',
            Keypad::Subtract => '-',
            Keypad::Add => '+',
            Keypad::Equal => '=',
            Keypad::Enter => '\r',
        }
    }

    /// The final byte of its `SS3` form in application keypad mode, as
    /// xterm and ghostty send it. Equal has none there and types `=`.
    fn ss3_final(self) -> Option<char> {
        let final_byte = match self {
            Keypad::Digit(digit) => char::from(b'p' + digit.min(9)),
            Keypad::Decimal => 'n',
            Keypad::Divide => 'o',
            Keypad::Multiply => 'j',
            Keypad::Subtract => 'm',
            Keypad::Add => 'k',
            Keypad::Enter => 'M',
            Keypad::Equal => return None,
        };
        Some(final_byte)
    }

    /// Kitty's code for it, from the protocol's private range.
    fn kitty_code(self) -> u32 {
        match self {
            Keypad::Digit(digit) => 57399 + u32::from(digit.min(9)),
            Keypad::Decimal => 57409,
            Keypad::Divide => 57410,
            Keypad::Multiply => 57411,
            Keypad::Subtract => 57412,
            Keypad::Add => 57413,
            Keypad::Enter => 57414,
            Keypad::Equal => 57415,
        }
    }
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
    /// DECKPAM. Numpad keys send `SS3` codes instead of what they type.
    pub app_keypad: bool,
    pub kitty: KittyFlags,
    /// `CSI > 4 ; n m`, xterm's `modifyOtherKeys`. Level 1 reports only the
    /// chords legacy has no bytes for (ctrl+`;`), so ctrl+a is still `^A`;
    /// level 2 reports every modified key that types, as `CSI 27 ; m ; c ~`.
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
        Key::Keypad(key) => key.kitty_code(),
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
    // Writing into a String can't fail.
    let _ = write!(out, ";{}", mods.param());
    if report_event {
        let _ = write!(out, ":{}", action.kitty_param());
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
        let _ = write!(out, "{number}");
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

    let _ = write!(out, "{}", kitty_codepoint(key));
    if let Some(shifted) = shifted {
        let _ = write!(out, ":{shifted}");
    }
    let associated = associated_text(key, mods, action, text, ctx);
    if associated.is_some() && mods.is_empty() && !has_event_param(action, ctx.kitty) {
        // Text is the third parameter, so the modifiers slot stays, empty,
        // as kitty and ghostty write it: `CSI 97 ; ; 97 u`.
        out.push(';');
    } else {
        push_mods_and_event(&mut out, mods, action, ctx.kitty);
    }
    if let Some(text) = associated {
        let typed = text.chars().filter(|c| !c.is_control());
        for (index, c) in typed.enumerate() {
            let separator = if index == 0 { ';' } else { ':' };
            let _ = write!(out, "{separator}{}", u32::from(c));
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
fn associated_text<'a>(
    key: Key,
    mods: Modifiers,
    action: KeyAction,
    text: Option<&'a str>,
    ctx: &EncodeContext,
) -> Option<&'a str> {
    let chord = mods.intersects(Modifiers::CTRL | Modifiers::SUPER)
        || (mods.contains(Modifiers::ALT) && ctx.alt_is_meta);
    if !ctx.kitty.contains(KittyFlags::REPORT_ASSOCIATED_TEXT)
        || !ctx.kitty.contains(KittyFlags::REPORT_ALL_AS_ESC)
        || action == KeyAction::Release
        || chord
        || !matches!(key, Key::Char(_) | Key::Keypad(_))
    {
        return None;
    }
    // The caller writes it without the control characters; there has to be
    // something left.
    text.filter(|text| text.chars().any(|c| !c.is_control()))
}

/// `CSI 27 ; mods ; codepoint ~`, xterm's unambiguous form for a modified key.
fn encode_modify_other_keys(codepoint: u32, mods: Modifiers) -> Vec<u8> {
    format!("\x1b[27;{};{}~", mods.param(), codepoint).into_bytes()
}

/// A numpad key outside kitty mode. In application keypad mode it sends its
/// `SS3` code, with the modifier parameter when one is held (`ESC O 5 q` for
/// ctrl+1); otherwise it types its symbol, whatever is held, and Enter is
/// Enter. `modifyOtherKeys` leaves the numpad alone, as in ghostty.
fn encode_keypad(
    key: Keypad,
    mods: Modifiers,
    text: Option<&str>,
    ctx: &EncodeContext,
) -> Vec<u8> {
    if ctx.app_keypad {
        if let Some(final_byte) = key.ss3_final() {
            return if mods.is_empty() {
                format!("\x1bO{final_byte}").into_bytes()
            } else {
                format!("\x1bO{}{final_byte}", mods.param()).into_bytes()
            };
        }
    }
    if key == Keypad::Enter {
        return b"\r".to_vec();
    }
    match text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control)) {
        Some(text) => text.as_bytes().to_vec(),
        None => key.symbol().to_string().into_bytes(),
    }
}

/// The codepoint `modifyOtherKeys` reports for a text key: the character it
/// typed when that is one printable character (`A` for shift+a, as xterm
/// reports the keysym), else the key itself.
fn modify_other_keys_codepoint(c: char, text: Option<&str>) -> u32 {
    let mut chars = text.unwrap_or("").chars();
    match (chars.next(), chars.next()) {
        (Some(typed), None) if !typed.is_control() => typed as u32,
        _ => c as u32,
    }
}

/// Whether `modifyOtherKeys` reports this text key as `CSI 27`. Level 2
/// follows xterm's `ModifyOtherKeys` (and ghostty): any modifier on a key in
/// the control range (`@` to DEL, letters included), anything but shift
/// alone on the rest, and shift+space. Level 1 only reports the chords legacy
/// can't express: ctrl on a key with no control byte, such as ctrl+`;`.
fn modify_other_keys_reports(
    level: u8,
    c: char,
    codepoint: u32,
    mods: Modifiers,
) -> bool {
    if mods.is_empty() {
        return false;
    }
    if level >= 2 {
        let control_range = (0x40..=0x7f).contains(&codepoint);
        let beyond_shift = !mods.difference(Modifiers::SHIFT).is_empty();
        return control_range || beyond_shift || codepoint == u32::from(' ');
    }
    mods.contains(Modifiers::CTRL) && ctrl_byte(c).is_none()
}

/// Enter, Tab and Backspace under `modifyOtherKeys`, from ghostty's (and
/// xterm's) tables. Level 1 keeps the forms legacy already sends apart (alt
/// as an ESC prefix, shift+tab as back tab, backspace's bytes); everything
/// else, and every modified key at level 2, is `CSI 27 ; m ; code ~`.
/// ctrl+backspace stays ^H at both levels.
fn modify_other_keys_editing(key: Key, level: u8, mods: Modifiers) -> Option<Vec<u8>> {
    if mods.is_empty() || mods.contains(Modifiers::SUPER) {
        return None;
    }
    let (code, legacy_at_level_1) = match key {
        Key::Enter => (13, mods == Modifiers::ALT),
        Key::Tab => (9, mods == Modifiers::ALT || mods == Modifiers::SHIFT),
        Key::Backspace => (127, true),
        _ => return None,
    };
    if level < 2 && legacy_at_level_1 {
        return None;
    }
    if key == Key::Backspace && mods == Modifiers::CTRL {
        return None;
    }
    Some(encode_modify_other_keys(code, mods))
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

    if let Key::Keypad(keypad) = key {
        return Some(encode_keypad(keypad, mods, text, ctx));
    }

    let base: Vec<u8> = match key {
        Key::Enter => vec![if ctrl || mods.contains(Modifiers::SHIFT) {
            b'\n'
        } else {
            b'\r'
        }],
        Key::Tab => {
            if mods.contains(Modifiers::SHIFT) {
                return Some(b"\x1b[Z".to_vec());
            }
            vec![b'\t']
        }
        // ctrl+backspace is ^H, as in xterm and ghostty, so a shell can bind
        // it apart from backspace (usually to delete a word).
        Key::Backspace => vec![if ctrl { 0x08 } else { 0x7f }],
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
        | Key::F(_)
        | Key::Keypad(_) => return None,
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
    // Enter, Tab and Backspace keep their legacy bytes only unmodified, as
    // in kitty and ghostty: shift+enter, shift+tab and shift+backspace each
    // need a sequence of their own when the terminal asked to see modified
    // keys. The numpad is the same: its keys type unmodified, and are
    // reported by their own codes otherwise. Keypad Enter types no text, so
    // it is always its own code.
    if mods.contains(Modifiers::SHIFT)
        && matches!(key, Key::Enter | Key::Tab | Key::Backspace | Key::Keypad(_))
    {
        return true;
    }
    matches!(key, Key::Escape | Key::Keypad(Keypad::Enter))
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
        let typed_text = matches!(key, Key::Char(_) | Key::Keypad(_))
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

        // An unmodified numpad key types under kitty's lower levels, whatever
        // the keypad mode: DECKPAM is the legacy encoding's, and kitty gives
        // the numpad codes of its own for when they're asked for.
        if let Key::Keypad(keypad) = key {
            let typing = EncodeContext {
                app_keypad: false,
                ..*ctx
            };
            return Some(encode_keypad(keypad, mods, text, &typing));
        }
    }

    // Text keys with a modifier the legacy form mangles, when the program has
    // asked for the unambiguous form.
    if let Some(level) = ctx.modify_other_keys.filter(|level| *level >= 1) {
        if let Key::Char(c) = key {
            let codepoint = modify_other_keys_codepoint(c, text);
            // Alt that isn't meta is the Mac's Option typing a character,
            // not a chord, even when the host didn't mark it spent; ghostty
            // drops it here the same way.
            let chord_mods = if ctx.alt_is_meta {
                mods
            } else {
                mods.difference(Modifiers::ALT)
            };
            if !chord_mods.contains(Modifiers::SUPER)
                && modify_other_keys_reports(level, c, codepoint, chord_mods)
            {
                return Some(encode_modify_other_keys(codepoint, chord_mods));
            }
        }
        if let Some(bytes) = modify_other_keys_editing(key, level, mods) {
            return Some(bytes);
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
    fn ctrl_enter_is_a_linefeed_in_legacy_mode() {
        assert_eq!(encode(&press(Key::Enter), &legacy()), Some(b"\r".to_vec()));
        assert_eq!(
            encode(&with_mods(Key::Enter, Modifiers::CTRL), &legacy()),
            Some(b"\n".to_vec())
        );
        assert_eq!(
            encode(
                &with_mods(Key::Enter, Modifiers::CTRL),
                &kitty(KittyFlags::DISAMBIGUATE)
            ),
            Some(b"\x1b[13;5u".to_vec())
        );
    }

    #[test]
    fn shift_enter_is_a_linefeed_in_legacy_mode() {
        assert_eq!(
            encode(&with_mods(Key::Enter, Modifiers::SHIFT), &legacy()),
            Some(b"\n".to_vec())
        );
    }

    #[test]
    fn shift_enter_is_disambiguated_by_kitty() {
        assert_eq!(
            encode(
                &with_mods(Key::Enter, Modifiers::SHIFT),
                &kitty(KittyFlags::DISAMBIGUATE)
            ),
            Some(b"\x1b[13;2u".to_vec())
        );
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

    fn mok(level: u8) -> EncodeContext {
        EncodeContext {
            modify_other_keys: Some(level),
            ..legacy()
        }
    }

    fn keypad(symbol: char) -> KeyEvent {
        let key = Keypad::from_symbol(symbol).expect("a numpad symbol");
        KeyEvent {
            key: Some(Key::Keypad(key)),
            text: (symbol != '\r').then(|| symbol.to_string()),
            ..Default::default()
        }
    }

    fn app_keypad() -> EncodeContext {
        EncodeContext {
            app_keypad: true,
            ..legacy()
        }
    }

    #[test]
    fn modify_other_keys_level_1_keeps_control_bytes() {
        // Level 1 leaves the chords legacy already sends alone.
        assert_eq!(
            encode(&typed('a', Modifiers::CTRL, "a"), &mok(1)),
            Some(vec![0x01])
        );
        assert_eq!(
            encode(&typed('2', Modifiers::CTRL, "2"), &mok(1)),
            Some(vec![0x00])
        );
        assert_eq!(
            encode(&typed('a', Modifiers::ALT, "a"), &mok(1)),
            Some(b"\x1ba".to_vec())
        );
        assert_eq!(
            encode(&typed('a', Modifiers::SHIFT, "A"), &mok(1)),
            Some(b"A".to_vec())
        );
    }

    #[test]
    fn modify_other_keys_level_1_reports_what_legacy_cannot() {
        // ctrl+; and ctrl+1 have no control byte: level 1 is how a program
        // gets to see them.
        assert_eq!(
            encode(&typed(';', Modifiers::CTRL, ";"), &mok(1)),
            Some(b"\x1b[27;5;59~".to_vec())
        );
        assert_eq!(
            encode(&typed('1', Modifiers::CTRL, "1"), &mok(1)),
            Some(b"\x1b[27;5;49~".to_vec())
        );
    }

    #[test]
    fn modify_other_keys_level_2_reports_modified_keys() {
        let ctx = mok(2);
        assert_eq!(
            encode(&typed('a', Modifiers::CTRL, "a"), &ctx),
            Some(b"\x1b[27;5;97~".to_vec())
        );
        assert_eq!(
            encode(&typed('a', Modifiers::ALT, "a"), &ctx),
            Some(b"\x1b[27;3;97~".to_vec())
        );
        // Letters are in the control range, so shift alone is reported, with
        // the character typed, as xterm reports the keysym.
        assert_eq!(
            encode(&typed('a', Modifiers::SHIFT, "A"), &ctx),
            Some(b"\x1b[27;2;65~".to_vec())
        );
        assert_eq!(
            encode(&typed(' ', Modifiers::SHIFT, " "), &ctx),
            Some(b"\x1b[27;2;32~".to_vec())
        );
    }

    #[test]
    fn modify_other_keys_level_2_leaves_shifted_symbols_and_typing_alone() {
        let ctx = mok(2);
        // Shift alone on a symbol outside the control range types it.
        assert_eq!(
            encode(&typed('1', Modifiers::SHIFT, "!"), &ctx),
            Some(b"!".to_vec())
        );
        assert_eq!(
            encode(&typed('a', Modifiers::empty(), "a"), &ctx),
            Some(b"a".to_vec())
        );
        // Option typing © on a Mac isn't a chord.
        let mut option = typed('g', Modifiers::ALT, "\u{a9}");
        option.consumed_mods = Modifiers::ALT;
        assert_eq!(encode(&option, &ctx), Some("\u{a9}".as_bytes().to_vec()));
        // Super is the platform's, here as everywhere.
        assert_eq!(encode(&typed('c', Modifiers::SUPER, "c"), &ctx), None);
    }

    #[test]
    fn modify_other_keys_option_typing_is_text_without_consumed_alt() {
        // A host that doesn't mark Option spent: with alt not meta, `©` is
        // still what option+g typed, not an alt chord.
        let ctx = EncodeContext {
            alt_is_meta: false,
            ..mok(2)
        };
        let option = typed('g', Modifiers::ALT, "\u{a9}");
        assert_eq!(encode(&option, &ctx), Some("\u{a9}".as_bytes().to_vec()));
        // ctrl alongside still makes it a chord, reported without the alt.
        let mut ctrl = typed('g', Modifiers::ALT | Modifiers::CTRL, "g");
        ctrl.text = Some("g".into());
        assert_eq!(encode(&ctrl, &ctx), Some(b"\x1b[27;5;103~".to_vec()));
        // With alt as meta it is a chord.
        let meta = typed('g', Modifiers::ALT, "g");
        assert_eq!(encode(&meta, &mok(2)), Some(b"\x1b[27;3;103~".to_vec()));
    }

    #[test]
    fn modify_other_keys_level_1_editing_keys() {
        let ctx = mok(1);
        let cases: [(Key, Modifiers, &[u8]); 9] = [
            (Key::Enter, Modifiers::SHIFT, b"\x1b[27;2;13~"),
            (Key::Enter, Modifiers::CTRL, b"\x1b[27;5;13~"),
            (Key::Enter, Modifiers::ALT, b"\x1b\r"),
            (Key::Tab, Modifiers::SHIFT, b"\x1b[Z"),
            (Key::Tab, Modifiers::ALT, b"\x1b\t"),
            (Key::Tab, Modifiers::CTRL, b"\x1b[27;5;9~"),
            (Key::Tab, Modifiers::ALT | Modifiers::SHIFT, b"\x1b[27;4;9~"),
            (Key::Backspace, Modifiers::SHIFT, b"\x7f"),
            (Key::Backspace, Modifiers::CTRL, b"\x08"),
        ];
        for (key, mods, expected) in cases {
            assert_eq!(
                encode(&with_mods(key, mods), &ctx),
                Some(expected.to_vec()),
                "{key:?} {mods:?}"
            );
        }
        assert_eq!(
            encode(&with_mods(Key::Backspace, Modifiers::ALT), &ctx),
            Some(b"\x1b\x7f".to_vec())
        );
    }

    #[test]
    fn modify_other_keys_level_2_editing_keys() {
        let ctx = mok(2);
        let cases: [(Key, Modifiers, &[u8]); 7] = [
            (Key::Enter, Modifiers::ALT, b"\x1b[27;3;13~"),
            (Key::Enter, Modifiers::SHIFT, b"\x1b[27;2;13~"),
            (Key::Tab, Modifiers::SHIFT, b"\x1b[27;2;9~"),
            (Key::Tab, Modifiers::ALT, b"\x1b[27;3;9~"),
            (Key::Backspace, Modifiers::SHIFT, b"\x1b[27;2;127~"),
            (
                Key::Backspace,
                Modifiers::CTRL | Modifiers::SHIFT,
                b"\x1b[27;6;127~",
            ),
            // ctrl+backspace is ^H at every level.
            (Key::Backspace, Modifiers::CTRL, b"\x08"),
        ];
        for (key, mods, expected) in cases {
            assert_eq!(
                encode(&with_mods(key, mods), &ctx),
                Some(expected.to_vec()),
                "{key:?} {mods:?}"
            );
        }
        // Unmodified, they stay what they are.
        assert_eq!(encode(&press(Key::Enter), &ctx), Some(b"\r".to_vec()));
        assert_eq!(encode(&press(Key::Tab), &ctx), Some(b"\t".to_vec()));
        assert_eq!(encode(&press(Key::Backspace), &ctx), Some(vec![0x7f]));
    }

    #[test]
    fn ctrl_backspace_is_ctrl_h() {
        assert_eq!(
            encode(&with_mods(Key::Backspace, Modifiers::CTRL), &legacy()),
            Some(vec![0x08])
        );
        assert_eq!(
            encode(
                &with_mods(Key::Backspace, Modifiers::CTRL | Modifiers::ALT),
                &legacy()
            ),
            Some(b"\x1b\x08".to_vec())
        );
        assert_eq!(encode(&press(Key::Backspace), &legacy()), Some(vec![0x7f]));
    }

    #[test]
    fn kitty_disambiguates_shifted_tab_and_backspace() {
        let ctx = kitty(KittyFlags::DISAMBIGUATE);
        assert_eq!(
            encode(&with_mods(Key::Tab, Modifiers::SHIFT), &ctx),
            Some(b"\x1b[9;2u".to_vec())
        );
        assert_eq!(
            encode(&with_mods(Key::Backspace, Modifiers::SHIFT), &ctx),
            Some(b"\x1b[127;2u".to_vec())
        );
        assert_eq!(
            encode(&with_mods(Key::Enter, Modifiers::SHIFT), &ctx),
            Some(b"\x1b[13;2u".to_vec())
        );
        // Unmodified they keep legacy bytes, so a shell still works.
        assert_eq!(encode(&press(Key::Tab), &ctx), Some(b"\t".to_vec()));
        assert_eq!(encode(&press(Key::Backspace), &ctx), Some(vec![0x7f]));
    }

    #[test]
    fn held_shift_tab_repeats_like_its_press() {
        let ctx = event_types();
        let repeat = repeated(with_mods(Key::Tab, Modifiers::SHIFT));
        assert_eq!(encode(&repeat, &ctx), Some(b"\x1b[9;2:2u".to_vec()));
    }

    #[test]
    fn keypad_symbols_round_trip() {
        for symbol in "0123456789./*-+=\r".chars() {
            let key = Keypad::from_symbol(symbol).expect("numpad symbol");
            assert_eq!(key.symbol(), symbol);
        }
        assert_eq!(Keypad::from_symbol('a'), None);
        assert_eq!(Keypad::from_symbol('\n'), None);
    }

    #[test]
    fn keypad_types_in_normal_mode() {
        assert_eq!(encode(&keypad('1'), &legacy()), Some(b"1".to_vec()));
        assert_eq!(encode(&keypad('+'), &legacy()), Some(b"+".to_vec()));
        assert_eq!(encode(&keypad('\r'), &legacy()), Some(b"\r".to_vec()));
        // Modifiers don't change what a numpad key types here, as in ghostty.
        let mut ctrl = keypad('1');
        ctrl.mods = Modifiers::CTRL;
        assert_eq!(encode(&ctrl, &legacy()), Some(b"1".to_vec()));
    }

    #[test]
    fn keypad_application_mode_sends_ss3() {
        let ctx = app_keypad();
        let cases = [
            ('0', "\x1bOp"),
            ('1', "\x1bOq"),
            ('9', "\x1bOy"),
            ('.', "\x1bOn"),
            ('/', "\x1bOo"),
            ('*', "\x1bOj"),
            ('-', "\x1bOm"),
            ('+', "\x1bOk"),
            ('\r', "\x1bOM"),
        ];
        for (symbol, expected) in cases {
            assert_eq!(
                encode(&keypad(symbol), &ctx),
                Some(expected.as_bytes().to_vec()),
                "{symbol:?}"
            );
        }
        // Equal has no SS3 form there; it types.
        assert_eq!(encode(&keypad('='), &ctx), Some(b"=".to_vec()));
    }

    #[test]
    fn keypad_application_mode_carries_modifiers() {
        let mut ctrl = keypad('1');
        ctrl.mods = Modifiers::CTRL;
        assert_eq!(encode(&ctrl, &app_keypad()), Some(b"\x1bO5q".to_vec()));
        // modifyOtherKeys level 2 leaves the numpad to the keypad mode, as
        // ghostty's tests have it.
        let both = EncodeContext {
            modify_other_keys: Some(2),
            ..app_keypad()
        };
        assert_eq!(encode(&ctrl, &both), Some(b"\x1bO5q".to_vec()));
        assert_eq!(encode(&ctrl, &mok(2)), Some(b"1".to_vec()));
    }

    #[test]
    fn keypad_in_kitty_mode() {
        // Below report-all an unmodified numpad digit types, even in
        // application keypad mode.
        let ctx = EncodeContext {
            app_keypad: true,
            ..kitty(KittyFlags::DISAMBIGUATE)
        };
        assert_eq!(encode(&keypad('1'), &ctx), Some(b"1".to_vec()));
        // Modified, or Enter, it is its own kitty code.
        let mut shifted = keypad('1');
        shifted.mods = Modifiers::SHIFT;
        assert_eq!(encode(&shifted, &ctx), Some(b"\x1b[57400;2u".to_vec()));
        assert_eq!(encode(&keypad('\r'), &ctx), Some(b"\x1b[57414u".to_vec()));
        // Report-all with text: its code, then what it typed.
        assert_eq!(
            encode(&keypad('5'), &yazi()),
            Some(b"\x1b[57404;;53u".to_vec())
        );
        assert_eq!(
            encode(&keypad('='), &yazi()),
            Some(b"\x1b[57415;;61u".to_vec())
        );
    }

    #[test]
    fn held_keypad_digit_repeats_as_text() {
        let repeat = repeated(keypad('7'));
        assert_eq!(encode(&repeat, &event_types()), Some(b"7".to_vec()));
    }

    #[test]
    fn associated_text_needs_report_all() {
        // Without report-all, text keys arrive as plain text anyway.
        let ctx = kitty(KittyFlags::DISAMBIGUATE | KittyFlags::REPORT_ASSOCIATED_TEXT);
        let event = typed('g', Modifiers::SHIFT, "G");
        assert_eq!(encode(&event, &ctx), Some(b"G".to_vec()));
    }
}
