//! The application menu exported to the desktop's menu bar on Wayland
//! (`window.menubar = "Auto"`).
//!
//! The entries are a subset of the command palette's and run through the
//! same code, so an entry does to the window it belongs to exactly what the
//! palette would. The shortcut shown beside an entry is read from the key
//! bindings in use; the desktop only displays it.

use crate::bindings::{Action, BindingKey, BindingMode, KeyBinding};
use crate::renderer::command_palette::PaletteAction;
use rio_backend::config::window::Menubar;
use rio_backend::config::Config;
use rio_backend::event::{EventProxy, RioEvent, RioEventType};
use rio_window::keyboard::{Key, ModifiersState, NamedKey};
use rio_window::platform::wayland::{AppMenuItem, WindowExtAppMenu};
use rio_window::window::Window;

/// The menus, in the order they are shown. `None` is a separator.
const MENUS: &[(&str, &[Option<PaletteAction>])] = &[
    (
        "File",
        &[
            Some(PaletteAction::WindowCreateNew),
            Some(PaletteAction::TabCreate),
            None,
            Some(PaletteAction::SplitRight),
            Some(PaletteAction::SplitDown),
            None,
            Some(PaletteAction::CloseCurrentSplitOrTab),
            Some(PaletteAction::Quit),
        ],
    ),
    (
        "Edit",
        &[
            Some(PaletteAction::Copy),
            Some(PaletteAction::Paste),
            None,
            Some(PaletteAction::SearchForward),
            None,
            Some(PaletteAction::ConfigEditor),
        ],
    ),
    (
        "View",
        &[
            Some(PaletteAction::IncreaseFontSize),
            Some(PaletteAction::DecreaseFontSize),
            Some(PaletteAction::ResetFontSize),
            None,
            Some(PaletteAction::ToggleFullscreen),
        ],
    ),
];

fn entries() -> impl Iterator<Item = PaletteAction> {
    MENUS
        .iter()
        .flat_map(|(_, entries)| entries.iter().flatten().copied())
}

/// The command behind the id the menu bar sent back: an entry's position
/// among all the entries, counted from 1.
pub fn action(id: u32) -> Option<PaletteAction> {
    entries().nth((id as usize).checked_sub(1)?)
}

fn binding_action(action: PaletteAction) -> Option<Action> {
    Some(match action {
        PaletteAction::WindowCreateNew => Action::WindowCreateNew,
        PaletteAction::TabCreate => Action::TabCreateNew,
        PaletteAction::SplitRight => Action::SplitRight,
        PaletteAction::SplitDown => Action::SplitDown,
        PaletteAction::CloseCurrentSplitOrTab => Action::CloseCurrentSplitOrTab,
        PaletteAction::Quit => Action::Quit,
        PaletteAction::Copy => Action::Copy,
        PaletteAction::Paste => Action::Paste,
        PaletteAction::SearchForward => Action::SearchForward,
        PaletteAction::ConfigEditor => Action::ConfigEditor,
        PaletteAction::IncreaseFontSize => Action::IncreaseFontSize,
        PaletteAction::DecreaseFontSize => Action::DecreaseFontSize,
        PaletteAction::ResetFontSize => Action::ResetFontSize,
        PaletteAction::ToggleFullscreen => Action::ToggleFullscreen,
        _ => return None,
    })
}

/// The name a menu bar knows a key by, which is its X11 keysym name.
fn key_name(key: &Key) -> Option<String> {
    let name = match key {
        Key::Character(text) => {
            let mut chars = text.chars();
            let character = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            match character {
                ',' => "comma",
                '.' => "period",
                '/' => "slash",
                ';' => "semicolon",
                '\'' => "apostrophe",
                '`' => "grave",
                '[' => "bracketleft",
                ']' => "bracketright",
                '\\' => "backslash",
                '-' => "minus",
                '=' => "equal",
                '+' => "plus",
                ' ' => "space",
                character if character.is_alphanumeric() => {
                    return Some(character.to_lowercase().collect())
                }
                _ => return None,
            }
        }
        Key::Named(named) => match named {
            NamedKey::Enter => "Return",
            NamedKey::Tab => "Tab",
            NamedKey::Space => "space",
            NamedKey::Escape => "Escape",
            NamedKey::Backspace => "BackSpace",
            NamedKey::Delete => "Delete",
            NamedKey::Insert => "Insert",
            NamedKey::Home => "Home",
            NamedKey::End => "End",
            NamedKey::PageUp => "Page_Up",
            NamedKey::PageDown => "Page_Down",
            NamedKey::ArrowUp => "Up",
            NamedKey::ArrowDown => "Down",
            NamedKey::ArrowLeft => "Left",
            NamedKey::ArrowRight => "Right",
            NamedKey::F1 => "F1",
            NamedKey::F2 => "F2",
            NamedKey::F3 => "F3",
            NamedKey::F4 => "F4",
            NamedKey::F5 => "F5",
            NamedKey::F6 => "F6",
            NamedKey::F7 => "F7",
            NamedKey::F8 => "F8",
            NamedKey::F9 => "F9",
            NamedKey::F10 => "F10",
            NamedKey::F11 => "F11",
            NamedKey::F12 => "F12",
            _ => return None,
        },
        _ => return None,
    };
    Some(name.to_owned())
}

/// The keys of the binding that runs `action` in a plain terminal, as a
/// menu bar wants them: modifiers first, then the key.
///
/// The last binding wins, since the bindings of the configuration file
/// come after the defaults. A binding without modifiers is passed over (a
/// dedicated `Copy` key, say), and so is one that only applies in a mode
/// such as Vi.
fn shortcut(bindings: &[KeyBinding], action: &Action) -> Vec<String> {
    bindings
        .iter()
        .rev()
        .filter(|binding| binding.action == *action)
        .filter(|binding| {
            binding.mode == BindingMode::empty() && !binding.mods.is_empty()
        })
        .find_map(|binding| {
            let BindingKey::Keycode { key, .. } = &binding.trigger else {
                return None;
            };
            let key = key_name(key)?;
            let mut keys = Vec::with_capacity(4);
            for (modifier, name) in [
                (ModifiersState::CONTROL, "Control"),
                (ModifiersState::SHIFT, "Shift"),
                (ModifiersState::ALT, "Alt"),
                (ModifiersState::SUPER, "Super"),
            ] {
                if binding.mods.contains(modifier) {
                    keys.push(name.to_owned());
                }
            }
            keys.push(key);
            Some(keys)
        })
        .unwrap_or_default()
}

/// Whether the configuration leaves a menu entry anything to do.
fn enabled(action: PaletteAction, config: &Config) -> bool {
    match action {
        PaletteAction::TabCreate => config.navigation.has_navigation_key_bindings(),
        PaletteAction::SplitRight | PaletteAction::SplitDown => {
            config.navigation.use_split
        }
        _ => true,
    }
}

/// The label of a menu entry. The palette's own, save where a menu has a
/// more usual word for it.
fn label(action: PaletteAction) -> &'static str {
    match action {
        PaletteAction::CloseCurrentSplitOrTab => "Close",
        PaletteAction::SearchForward => "Find…",
        PaletteAction::ConfigEditor => "Settings…",
        PaletteAction::ToggleFullscreen => "Full Screen",
        _ => action.title(),
    }
}

fn items(config: &Config, bindings: &[KeyBinding]) -> Vec<AppMenuItem> {
    let mut id = 0;
    MENUS
        .iter()
        .map(|(menu, entries)| {
            let children = entries
                .iter()
                .map(|entry| {
                    let Some(action) = *entry else {
                        return AppMenuItem::separator();
                    };
                    id += 1;
                    let keys = binding_action(action)
                        .map(|bound| shortcut(bindings, &bound))
                        .unwrap_or_default();
                    AppMenuItem::new(id, label(action))
                        .with_shortcut(keys)
                        .with_enabled(enabled(action, config))
                })
                .collect();
            AppMenuItem::submenu(*menu, children)
        })
        .collect()
}

/// Export the menu of `window`, replace it after a configuration change, or
/// withdraw it, as `window.menubar` says. Without a menu bar to show it
/// (X11, or a compositor that has no application menu protocol) nothing is
/// exported and the window behaves as with `menubar = "Disabled"`.
pub fn sync(
    window: &Window,
    config: &Config,
    bindings: &[KeyBinding],
    event_proxy: &EventProxy,
) {
    if config.window.menubar != Menubar::Auto {
        window.remove_app_menu();
        return;
    }

    let event_proxy = event_proxy.clone();
    let window_id = window.id().into();
    let exported = window.set_app_menu(items(config, bindings), move |id| {
        event_proxy.send_event(RioEventType::Rio(RioEvent::AppMenuAction(id)), window_id);
    });
    if !exported {
        tracing::debug!("no menu bar to export the application menu to");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rio_window::keyboard::KeyLocation;

    fn binding(key: Key, mods: ModifiersState, action: Action) -> KeyBinding {
        KeyBinding {
            mods,
            action,
            mode: BindingMode::empty(),
            notmode: BindingMode::empty(),
            trigger: BindingKey::Keycode {
                key,
                location: KeyLocation::Standard,
            },
        }
    }

    #[test]
    fn every_id_names_its_entry() {
        let menus = items(&Config::default(), &[]);
        let mut seen = 0;
        for entry in menus
            .iter()
            .flat_map(|menu| &menu.children)
            .filter(|entry| !entry.separator)
        {
            seen += 1;
            assert_eq!(entry.id, seen);
            let action = action(entry.id).unwrap();
            assert_eq!(entry.label, label(action));
            assert!(binding_action(action).is_some(), "{action:?}");
        }
        assert_eq!(action(0), None);
        assert_eq!(action(seen + 1), None);
    }

    #[test]
    fn the_shortcut_is_the_last_plain_binding_with_modifiers() {
        let mut vi = binding(
            Key::Character("y".into()),
            ModifiersState::CONTROL,
            Action::Copy,
        );
        vi.mode = BindingMode::VI;
        let bindings = [
            binding(
                Key::Character("c".into()),
                ModifiersState::SUPER,
                Action::Copy,
            ),
            binding(
                Key::Character("C".into()),
                ModifiersState::SHIFT | ModifiersState::CONTROL,
                Action::Copy,
            ),
            binding(
                Key::Named(NamedKey::Copy),
                ModifiersState::empty(),
                Action::Copy,
            ),
            vi,
        ];
        assert_eq!(
            shortcut(&bindings, &Action::Copy),
            ["Control", "Shift", "c"]
        );
        assert!(shortcut(&bindings, &Action::Paste).is_empty());

        let comma = [binding(
            Key::Character(",".into()),
            ModifiersState::ALT,
            Action::Copy,
        )];
        assert_eq!(shortcut(&comma, &Action::Copy), ["Alt", "comma"]);
    }
}
