bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Modifiers: u8 {
        const SHIFT = 1 << 0;
        const CTRL  = 1 << 1;
        const ALT   = 1 << 2;
        const SUPER = 1 << 3;
    }
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub mods: Modifiers,
}

impl KeyEvent {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            mods: Modifiers::empty(),
        }
    }
}

fn mods_param(mods: Modifiers) -> u8 {
    let mut value = 1;
    if mods.contains(Modifiers::SHIFT) {
        value += 1;
    }
    if mods.contains(Modifiers::ALT) {
        value += 2;
    }
    if mods.contains(Modifiers::CTRL) {
        value += 4;
    }
    value
}

fn csi_or_ss3(final_byte: char, mods: Modifiers, app_cursor: bool) -> Vec<u8> {
    if mods.is_empty() {
        if app_cursor {
            format!("\x1bO{final_byte}").into_bytes()
        } else {
            format!("\x1b[{final_byte}").into_bytes()
        }
    } else {
        format!("\x1b[1;{}{final_byte}", mods_param(mods)).into_bytes()
    }
}

fn tilde_seq(number: u8, mods: Modifiers) -> Vec<u8> {
    if mods.is_empty() {
        format!("\x1b[{number}~").into_bytes()
    } else {
        format!("\x1b[{number};{}~", mods_param(mods)).into_bytes()
    }
}

pub fn encode(event: KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    let mods = event.mods;
    let bytes = match event.key {
        Key::Char(c) => {
            if mods.contains(Modifiers::CTRL) {
                let lower = c.to_ascii_lowercase();
                if lower.is_ascii_alphabetic() {
                    let ctl = (lower as u8) & 0x1f;
                    if mods.contains(Modifiers::ALT) {
                        vec![0x1b, ctl]
                    } else {
                        vec![ctl]
                    }
                } else {
                    return None;
                }
            } else if mods.contains(Modifiers::ALT) {
                let mut buf = vec![0x1b];
                buf.extend(c.to_string().into_bytes());
                buf
            } else if mods.contains(Modifiers::SUPER) {
                return None;
            } else {
                c.to_string().into_bytes()
            }
        }
        Key::Enter => {
            if mods.contains(Modifiers::ALT) {
                vec![0x1b, b'\r']
            } else {
                vec![b'\r']
            }
        }
        Key::Tab => {
            if mods.contains(Modifiers::SHIFT) {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        Key::Backspace => {
            if mods.contains(Modifiers::ALT) {
                vec![0x1b, 0x7f]
            } else {
                vec![0x7f]
            }
        }
        Key::Escape => vec![0x1b],
        Key::Up => csi_or_ss3('A', mods, app_cursor),
        Key::Down => csi_or_ss3('B', mods, app_cursor),
        Key::Right => csi_or_ss3('C', mods, app_cursor),
        Key::Left => csi_or_ss3('D', mods, app_cursor),
        Key::Home => csi_or_ss3('H', mods, app_cursor),
        Key::End => csi_or_ss3('F', mods, app_cursor),
        Key::PageUp => tilde_seq(5, mods),
        Key::PageDown => tilde_seq(6, mods),
        Key::Insert => tilde_seq(2, mods),
        Key::Delete => tilde_seq(3, mods),
        Key::F(n) => match n {
            1 => csi_or_ss3('P', mods, true),
            2 => csi_or_ss3('Q', mods, true),
            3 => csi_or_ss3('R', mods, true),
            4 => csi_or_ss3('S', mods, true),
            5 => tilde_seq(15, mods),
            6 => tilde_seq(17, mods),
            7 => tilde_seq(18, mods),
            8 => tilde_seq(19, mods),
            9 => tilde_seq(20, mods),
            10 => tilde_seq(21, mods),
            11 => tilde_seq(23, mods),
            12 => tilde_seq(24, mods),
            _ => return None,
        },
    };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_char() {
        assert_eq!(
            encode(KeyEvent::new(Key::Char('a')), false),
            Some(vec![b'a'])
        );
    }

    #[test]
    fn ctrl_c() {
        let event = KeyEvent {
            key: Key::Char('c'),
            mods: Modifiers::CTRL,
        };
        assert_eq!(encode(event, false), Some(vec![0x03]));
    }

    #[test]
    fn arrows_follow_cursor_mode() {
        let up = KeyEvent::new(Key::Up);
        assert_eq!(encode(up, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode(up, true), Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn modified_arrow_ignores_app_mode() {
        let event = KeyEvent {
            key: Key::Up,
            mods: Modifiers::SHIFT,
        };
        assert_eq!(encode(event, true), Some(b"\x1b[1;2A".to_vec()));
    }
}
