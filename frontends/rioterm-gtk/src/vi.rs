//! Vi mode, as Rio has it: a cursor of its own over the scrollback,
//! moved and selecting with vi's keys, while the program gets none.

use crate::session::Session;
use gtk::gdk;
use rio_vt::crosswords::vi_mode::ViMotion;
use rio_vt::crosswords::Mode;
use rio_vt::selection::SelectionType;

pub enum Outcome {
    /// The key did its work in vi mode, or nothing.
    Handled,
    /// Copy the selection, then drop it.
    Copy,
    Find,
    /// Not a vi mode key: the terminal isn't in vi mode.
    Unhandled,
}

/// Ctrl+Shift+Space and Alt+Shift+Space switch vi mode.
pub fn is_toggle(keyval: gdk::Key, mods: gdk::ModifierType) -> bool {
    use gdk::ModifierType as M;
    keyval == gdk::Key::space
        && (mods == M::CONTROL_MASK | M::SHIFT_MASK
            || mods == M::ALT_MASK | M::SHIFT_MASK)
}

pub fn key(
    session: &Session,
    keyval: gdk::Key,
    mods: gdk::ModifierType,
    lines: i32,
) -> Outcome {
    use gdk::ModifierType as M;
    use rio_vt::crosswords::grid::Scroll;
    if !session.mode().contains(Mode::VI) {
        return Outcome::Unhandled;
    }
    let ctrl = mods.contains(M::CONTROL_MASK);
    let alt = mods.contains(M::ALT_MASK);
    let lower = keyval.to_lower();
    let c = keyval.to_unicode();
    if ctrl {
        match lower {
            gdk::Key::c => session.vi_toggle(),
            gdk::Key::b => session.scroll(Scroll::Delta(lines)),
            gdk::Key::f => session.scroll(Scroll::Delta(-lines)),
            gdk::Key::u => session.scroll(Scroll::Delta(lines / 2)),
            gdk::Key::d => session.scroll(Scroll::Delta(-lines / 2)),
            gdk::Key::y => session.scroll(Scroll::Delta(1)),
            gdk::Key::e => session.scroll(Scroll::Delta(-1)),
            gdk::Key::v => session.vi_select(SelectionType::Block),
            _ => {}
        }
        return Outcome::Handled;
    }
    if alt {
        if lower == gdk::Key::v {
            session.vi_select(SelectionType::Semantic);
        }
        return Outcome::Handled;
    }
    let motion = match keyval {
        gdk::Key::Up | gdk::Key::KP_Up => Some(ViMotion::Up),
        gdk::Key::Down | gdk::Key::KP_Down => Some(ViMotion::Down),
        gdk::Key::Left | gdk::Key::KP_Left => Some(ViMotion::Left),
        gdk::Key::Right | gdk::Key::KP_Right => Some(ViMotion::Right),
        _ => match c {
            Some('k') => Some(ViMotion::Up),
            Some('j') => Some(ViMotion::Down),
            Some('h') => Some(ViMotion::Left),
            Some('l') => Some(ViMotion::Right),
            Some('0') => Some(ViMotion::First),
            Some('$') => Some(ViMotion::Last),
            Some('^') => Some(ViMotion::FirstOccupied),
            Some('H') => Some(ViMotion::High),
            Some('M') => Some(ViMotion::Middle),
            Some('L') => Some(ViMotion::Low),
            Some('b') => Some(ViMotion::SemanticLeft),
            Some('w') => Some(ViMotion::SemanticRight),
            Some('e') => Some(ViMotion::SemanticRightEnd),
            Some('B') => Some(ViMotion::WordLeft),
            Some('W') => Some(ViMotion::WordRight),
            Some('E') => Some(ViMotion::WordRightEnd),
            Some('%') => Some(ViMotion::Bracket),
            _ => None,
        },
    };
    if let Some(motion) = motion {
        session.vi_motion(motion);
        return Outcome::Handled;
    }
    match (keyval, c) {
        (gdk::Key::Escape, _) => session.selection_clear(),
        (_, Some('i')) => {
            session.vi_toggle();
            session.scroll(Scroll::Bottom);
        }
        (_, Some('g')) => session.scroll(Scroll::Top),
        (_, Some('G')) => session.scroll(Scroll::Bottom),
        (_, Some('y')) => return Outcome::Copy,
        (_, Some('v')) => session.vi_select(SelectionType::Simple),
        (_, Some('V')) => session.vi_select(SelectionType::Lines),
        (_, Some('z')) => session.vi_center(),
        (_, Some('/')) => return Outcome::Find,
        _ => {}
    }
    Outcome::Handled
}
