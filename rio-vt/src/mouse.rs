//! Mouse input as the program sees it: button, motion and wheel reports
//! in the encodings it asked for, and what a wheel turn does when no
//! program takes it. Hosts hand over cell positions and modifiers; the
//! modes decide the rest.

use crate::crosswords::Mode;
use crate::key::Modifiers;

/// What a wheel turn does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wheel {
    /// Bytes for the program: mouse reports, or cursor keys on the
    /// alternate screen with alternate scroll on.
    Program(Vec<u8>),
    /// Nothing takes it: move the scrollback view.
    Scrollback,
}

/// A wheel turn of `lines` (positive up, towards history) over the cell
/// `col`, `row`. The program gets first claim: with mouse reporting on
/// it is a mouse event, on the alternate screen with alternate scroll on
/// it becomes cursor keys so pagers scroll, and otherwise it moves the
/// scrollback. Shift always means the scrollback.
pub fn wheel(mode: Mode, lines: i32, col: u16, row: u16, mods: Modifiers) -> Wheel {
    let shift = mods.contains(Modifiers::SHIFT);
    if lines == 0 || shift {
        return Wheel::Scrollback;
    }
    if mode.intersects(Mode::MOUSE_MODE) {
        // Wheel buttons are 64 (up) and 65 (down), with the modifier
        // bits every mouse report carries.
        let mut button = if lines > 0 { 64 } else { 65 };
        if mods.contains(Modifiers::ALT) {
            button += 8;
        }
        if mods.contains(Modifiers::CTRL) {
            button += 16;
        }
        let (sgr, utf8) = encodings(mode);
        let one = report(button, col, row, true, sgr, utf8);
        return Wheel::Program(one.repeat(lines.unsigned_abs() as usize));
    }
    if mode.contains(Mode::ALT_SCREEN) && mode.contains(Mode::ALTERNATE_SCROLL) {
        let seq: &[u8] = match (mode.contains(Mode::APP_CURSOR), lines > 0) {
            (true, true) => b"\x1bOA",
            (true, false) => b"\x1bOB",
            (false, true) => b"\x1b[A",
            (false, false) => b"\x1b[B",
        };
        return Wheel::Program(seq.repeat(lines.unsigned_abs() as usize));
    }
    Wheel::Scrollback
}

/// The report of a button press or release, when the program asked for
/// mouse events (DEC 1000/1002/1003, or X10/9). `button` is 0 left, 1
/// middle, 2 right. `None` when nothing grabs the mouse or shift is held
/// for a local selection.
pub fn button(
    mode: Mode,
    col: u16,
    row: u16,
    button: u8,
    pressed: bool,
    mods: Modifiers,
) -> Option<Vec<u8>> {
    if !mode.intersects(Mode::MOUSE_MODE) || mods.contains(Modifiers::SHIFT) {
        return None;
    }
    let x10 = mode.contains(Mode::MOUSE_REPORT_X10);
    // X10 (mode 9) reports only presses of the three main buttons, with
    // no modifiers and no release.
    if x10 && (!pressed || button > 2) {
        return None;
    }
    let mut encoded = button;
    if !x10 {
        if mods.contains(Modifiers::ALT) {
            encoded += 8;
        }
        if mods.contains(Modifiers::CTRL) {
            encoded += 16;
        }
    }
    let (sgr, utf8) = encodings(mode);
    Some(report(encoded, col, row, pressed, sgr, utf8))
}

/// The report of pointer motion for button-event (1002, a button held)
/// and any-event (1003, bare motion) modes. `button` is 0, 1 or 2 for
/// the button held during a drag, or 3 when none is.
pub fn motion(
    mode: Mode,
    col: u16,
    row: u16,
    button: u8,
    mods: Modifiers,
) -> Option<Vec<u8>> {
    if mods.contains(Modifiers::SHIFT) {
        return None;
    }
    let any = mode.contains(Mode::MOUSE_MOTION);
    // 1002 reports motion only while a button is down; 1003 reports all.
    let wanted = if button >= 3 {
        any
    } else {
        any || mode.contains(Mode::MOUSE_DRAG)
    };
    if !wanted {
        return None;
    }
    // The motion bit (32) rides on top of the button.
    let mut encoded = button.saturating_add(32);
    if mods.contains(Modifiers::ALT) {
        encoded += 8;
    }
    if mods.contains(Modifiers::CTRL) {
        encoded += 16;
    }
    let (sgr, utf8) = encodings(mode);
    Some(report(encoded, col, row, true, sgr, utf8))
}

fn encodings(mode: Mode) -> (bool, bool) {
    (
        mode.contains(Mode::SGR_MOUSE),
        mode.contains(Mode::UTF8_MOUSE),
    )
}

/// Encode one mouse report. SGR (`CSI < b ; x ; y M`) when the program
/// asked for it, else the original X10 form, whose coordinates are
/// offset by 32 and cannot exceed 223 without the UTF-8 extension.
pub fn report(
    button: u8,
    col: u16,
    row: u16,
    pressed: bool,
    sgr: bool,
    utf8: bool,
) -> Vec<u8> {
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    if sgr {
        // SGR keeps the real button and marks a release with lowercase `m`.
        let end = if pressed { 'M' } else { 'm' };
        return format!("\x1b[<{button};{x};{y}{end}").into_bytes();
    }
    // The X10/normal form has no release code: the button field collapses to
    // 3, but the modifier/motion bits (bit 3 and up) are kept, matching xterm.
    let encoded = if pressed { button } else { button | 3 };
    let mut out = vec![0x1b, b'[', b'M', 32u8.saturating_add(encoded)];
    for value in [x, y] {
        if utf8 && value >= 95 {
            // Two-byte UTF-8 for the extended range.
            let encoded = char::from_u32(32 + value as u32).unwrap_or('\u{20}');
            let mut buffer = [0u8; 4];
            out.extend_from_slice(encoded.encode_utf8(&mut buffer).as_bytes());
        } else {
            out.push(32u8.saturating_add(value.min(223) as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // SGR is the modern form; the X10 fallback offsets by 32.
    #[test]
    fn reports_encode_both_forms() {
        // SGR press keeps the button and ends in `M`; release ends in `m`.
        assert_eq!(
            report(64, 4, 2, true, true, false),
            b"\x1b[<64;5;3M".to_vec()
        );
        assert_eq!(
            report(0, 4, 2, false, true, false),
            b"\x1b[<0;5;3m".to_vec()
        );
        // X10 press offsets by 32; release collapses to button 3.
        assert_eq!(
            report(65, 0, 0, true, false, false),
            vec![0x1b, b'[', b'M', 32 + 65, 33, 33]
        );
        assert_eq!(
            report(0, 0, 0, false, false, false),
            vec![0x1b, b'[', b'M', 32 + 3, 33, 33]
        );
        // A legacy release keeps the modifier bits: ctrl+left release is
        // 16 | 3 = 19, not a bare 3.
        assert_eq!(
            report(16, 0, 0, false, false, false),
            vec![0x1b, b'[', b'M', 32 + 19, 33, 33]
        );
    }

    #[test]
    fn the_wheel_goes_to_the_program_first() {
        let none = Modifiers::empty();
        assert_eq!(wheel(Mode::empty(), 2, 0, 0, none), Wheel::Scrollback);
        assert_eq!(
            wheel(Mode::MOUSE_REPORT_CLICK | Mode::SGR_MOUSE, -2, 1, 1, none),
            Wheel::Program(b"\x1b[<65;2;2M\x1b[<65;2;2M".to_vec())
        );
        let pager = Mode::ALT_SCREEN | Mode::ALTERNATE_SCROLL;
        assert_eq!(
            wheel(pager, 1, 0, 0, none),
            Wheel::Program(b"\x1b[A".to_vec())
        );
        assert_eq!(
            wheel(pager | Mode::APP_CURSOR, -1, 0, 0, none),
            Wheel::Program(b"\x1bOB".to_vec())
        );
        assert_eq!(
            wheel(pager, 1, 0, 0, Modifiers::SHIFT),
            Wheel::Scrollback,
            "shift keeps the scrollback"
        );
    }

    #[test]
    fn clicks_and_motion_report_only_when_asked() {
        let none = Modifiers::empty();
        assert_eq!(button(Mode::empty(), 0, 0, 0, true, none), None);
        let click = Mode::MOUSE_REPORT_CLICK | Mode::SGR_MOUSE;
        assert_eq!(
            button(click, 4, 2, 0, true, Modifiers::CTRL),
            Some(b"\x1b[<16;5;3M".to_vec())
        );
        assert_eq!(button(click, 4, 2, 0, true, Modifiers::SHIFT), None);
        assert_eq!(motion(click, 0, 0, 0, none), None);
        assert_eq!(
            motion(click | Mode::MOUSE_DRAG, 0, 0, 0, none),
            Some(b"\x1b[<32;1;1M".to_vec())
        );
        assert_eq!(motion(click | Mode::MOUSE_DRAG, 0, 0, 3, none), None);
        assert_eq!(
            motion(click | Mode::MOUSE_MOTION, 0, 0, 3, none),
            Some(b"\x1b[<35;1;1M".to_vec())
        );
    }
}
