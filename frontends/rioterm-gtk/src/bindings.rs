//! Key bindings from the config's `[bindings]`, in Rio's syntax: a key,
//! its modifiers (`with`), an `action` or bytes to send (`esc`), and the
//! terminal modes it applies in (`mode`).

use gtk::gdk;
use rio_backend::config::bindings::KeyBinding;
use rio_vt::crosswords::Mode;

#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// A window or app action, by its detailed name.
    Action(String),
    Esc(String),
    Run(String, Vec<String>),
    Scroll(Scroll),
    ClearHistory,
    ToggleTheme,
    /// Label the matches of the hint rule at this index.
    Hint(usize),
    /// Leave the key to the terminal, over any shortcut on it.
    PassThrough,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scroll {
    Lines(i32),
    Pages(i32),
    HalfPages(i32),
    Top,
    Bottom,
    Prompt(bool),
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub key: gdk::Key,
    pub mods: gdk::ModifierType,
    pub act: Act,
    pub mode: Mode,
    pub not_mode: Mode,
}

impl Binding {
    pub fn matches(&self, key: gdk::Key, mods: gdk::ModifierType, mode: Mode) -> bool {
        self.key == key
            && self.mods == mods
            && mode.contains(self.mode)
            && !mode.intersects(self.not_mode)
    }

    /// The accelerator a shortcut on the same keys would use, for a
    /// binding that applies in every mode.
    pub fn accelerator(&self) -> Option<String> {
        if !self.mode.is_empty() || !self.not_mode.is_empty() {
            return None;
        }
        let mut accel = String::new();
        for (flag, name) in [
            (gdk::ModifierType::SHIFT_MASK, "<Shift>"),
            (gdk::ModifierType::CONTROL_MASK, "<Control>"),
            (gdk::ModifierType::ALT_MASK, "<Alt>"),
            (gdk::ModifierType::SUPER_MASK, "<Super>"),
        ] {
            if self.mods.contains(flag) {
                accel.push_str(name);
            }
        }
        accel.push_str(self.key.name()?.as_str());
        Some(accel)
    }
}

/// The config's bindings: hint rules' first, then `[bindings]`, which
/// win over them.
pub fn from_config(config: &rio_backend::config::Config) -> Vec<Binding> {
    let mut all: Vec<Binding> = config
        .hints
        .rules
        .iter()
        .enumerate()
        .filter_map(|(index, rule)| {
            let binding = rule.binding.as_ref()?;
            let parsed = parse_one(&KeyBinding {
                key: binding.key.clone(),
                with: binding.mods.join("|"),
                action: String::new(),
                esc: String::new(),
                mode: binding.mode.join("|"),
            });
            match parsed {
                Ok(mut parsed) => {
                    parsed.act = Act::Hint(index);
                    Some(parsed)
                }
                Err(_) => None,
            }
        })
        .collect();
    all.extend(parse(&config.bindings.keys));
    all
}

pub fn parse(bindings: &[KeyBinding]) -> Vec<Binding> {
    bindings
        .iter()
        .filter_map(|binding| match parse_one(binding) {
            Ok(parsed) => Some(parsed),
            Err(err) => {
                tracing::error!("key binding {:?}: {err}", binding.key);
                None
            }
        })
        .collect()
}

/// The keyval a key event is matched by: lowercase, so shift is only a
/// modifier.
pub fn normalize(key: gdk::Key) -> gdk::Key {
    key.to_lower()
}

fn parse_one(binding: &KeyBinding) -> Result<Binding, String> {
    let key = parse_key(&binding.key).ok_or("unknown key")?;
    let mut mods = gdk::ModifierType::empty();
    for name in binding.with.split('|') {
        mods |= match name.trim().to_lowercase().as_str() {
            "command" | "super" => gdk::ModifierType::SUPER_MASK,
            "shift" => gdk::ModifierType::SHIFT_MASK,
            "alt" | "option" => gdk::ModifierType::ALT_MASK,
            "control" | "ctrl" => gdk::ModifierType::CONTROL_MASK,
            _ => gdk::ModifierType::empty(),
        };
    }
    let act = if !binding.esc.is_empty() {
        Act::Esc(binding.esc.clone())
    } else if binding.action.is_empty() {
        Act::PassThrough
    } else {
        parse_action(&binding.action.to_lowercase())
            .ok_or_else(|| format!("unknown action {:?}", binding.action))?
    };
    let (mut mode, mut not_mode) = (Mode::empty(), Mode::empty());
    for name in binding.mode.split('|') {
        let name = name.trim().to_lowercase();
        let (negated, name) = match name.strip_prefix('~') {
            Some(name) => (true, name.to_string()),
            None => (false, name),
        };
        let flag = match name.as_str() {
            "appcursor" => Mode::APP_CURSOR,
            "appkeypad" => Mode::APP_KEYPAD,
            "alt" => Mode::ALT_SCREEN,
            "vi" => Mode::VI,
            _ => continue,
        };
        if negated {
            not_mode |= flag;
        } else {
            mode |= flag;
        }
    }
    Ok(Binding {
        key,
        mods,
        act,
        mode,
        not_mode,
    })
}

fn parse_key(name: &str) -> Option<gdk::Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.clone().next()) {
        use gtk::glib::translate::FromGlib;
        // SAFETY: any keyval GDK returns is a valid `gdk::Key`.
        let key = unsafe { gdk::Key::from_glib(gdk::unicode_to_keyval(c as u32)) };
        return Some(normalize(key));
    }
    let name = name.to_lowercase();
    let gdk_name = match name.as_str() {
        "esc" | "escape" => "Escape",
        "back" | "backspace" => "BackSpace",
        "return" | "enter" => "Return",
        "pageup" => "Page_Up",
        "pagedown" => "Page_Down",
        "colon" => "colon",
        "numpadenter" => "KP_Enter",
        "numpadadd" => "KP_Add",
        "numpadcomma" => "KP_Separator",
        "numpaddecimal" => "KP_Decimal",
        "numpaddivide" => "KP_Divide",
        "numpadequals" => "KP_Equal",
        "numpadsubtract" => "KP_Subtract",
        "numpadmultiply" => "KP_Multiply",
        other => {
            if let Some(digit) = other.strip_prefix("numpad") {
                return gdk::Key::from_name(format!("KP_{digit}"));
            }
            if let Some(n) = other.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                return gdk::Key::from_name(format!("F{n}"));
            }
            let mut name = other.to_string();
            if let Some(first) = name.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            return gdk::Key::from_name(name);
        }
    };
    gdk::Key::from_name(gdk_name)
}

fn parse_action(action: &str) -> Option<Act> {
    let window = |name: &str| Some(Act::Action(format!("win.{name}")));
    match action {
        "paste" => window("paste"),
        "copy" => window("copy"),
        "selectall" => window("select-all"),
        "searchforward" | "searchbackward" => window("find"),
        "clearhistory" => Some(Act::ClearHistory),
        "resetfontsize" => window("zoom-reset"),
        "increasefontsize" => window("zoom-in"),
        "decreasefontsize" => window("zoom-out"),
        "createwindow" => Some(Act::Action("app.new-window".into())),
        "quit" => Some(Act::Action("app.quit".into())),
        "createtab" => window("new-tab"),
        "movecurrenttabtoprev" => window("move-tab-left"),
        "movecurrenttabtonext" => window("move-tab-right"),
        "closetab" => window("close-tab"),
        "closesplitortab" => window("close"),
        "closeunfocusedtabs" => window("close-other-tabs"),
        "selectprevtab" => window("previous-tab"),
        "selectnexttab" => window("next-tab"),
        "selectlasttab" => window("select-tab(9)"),
        "splitright" => window("split-right"),
        "splitdown" => window("split-down"),
        "selectnextsplit" | "selectnextsplitortab" => window("next-split-or-tab"),
        "selectprevsplit" | "selectprevsplitortab" => window("previous-split-or-tab"),
        "movedividerup" => window("divider-up"),
        "movedividerdown" => window("divider-down"),
        "movedividerleft" => window("divider-left"),
        "movedividerright" => window("divider-right"),
        "togglefullscreen" => window("fullscreen"),
        "openconfigeditor" => window("open-config"),
        "toggleappearancetheme" => Some(Act::ToggleTheme),
        "scrollpageup" => Some(Act::Scroll(Scroll::Pages(1))),
        "scrollpagedown" => Some(Act::Scroll(Scroll::Pages(-1))),
        "scrollhalfpageup" => Some(Act::Scroll(Scroll::HalfPages(1))),
        "scrollhalfpagedown" => Some(Act::Scroll(Scroll::HalfPages(-1))),
        "scrolltotop" => Some(Act::Scroll(Scroll::Top)),
        "scrolltobottom" => Some(Act::Scroll(Scroll::Bottom)),
        "scrolltoprevprompt" => Some(Act::Scroll(Scroll::Prompt(false))),
        "scrolltonextprompt" => Some(Act::Scroll(Scroll::Prompt(true))),
        "receivechar" | "none" => Some(Act::PassThrough),
        _ => {
            let argument = |name: &str| {
                action
                    .strip_prefix(name)?
                    .strip_prefix('(')?
                    .strip_suffix(')')
                    .map(str::trim)
            };
            if let Some(n) = argument("selecttab") {
                let n: i32 = n.parse().ok()?;
                return window(&format!("select-tab({})", n + 1));
            }
            if let Some(lines) = argument("scroll") {
                return Some(Act::Scroll(Scroll::Lines(lines.parse().unwrap_or(1))));
            }
            if let Some(command) = argument("run") {
                let mut words = command.split(' ').filter(|w| !w.is_empty());
                let program = words.next()?.to_string();
                return Some(Act::Run(program, words.map(String::from).collect()));
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(key: &str, with: &str, action: &str, esc: &str, mode: &str) -> KeyBinding {
        KeyBinding {
            key: key.into(),
            with: with.into(),
            action: action.into(),
            esc: esc.into(),
            mode: mode.into(),
        }
    }

    #[test]
    fn parses_rio_binding_syntax() {
        let parsed = parse(&[
            binding("T", "control | shift", "SplitRight", "", ""),
            binding("Home", "", "", "\x1bOH", "appcursor | ~alt"),
            binding("PageUp", "shift", "ReceiveChar", "", ""),
            binding("1", "alt", "SelectTab(0)", "", ""),
            binding("r", "super", "Run(code -n)", "", ""),
            binding("q", "", "NoSuchAction", "", ""),
        ]);
        assert_eq!(parsed.len(), 5, "the unknown action is dropped");
        assert_eq!(parsed[0].key, gdk::Key::t);
        assert_eq!(
            parsed[0].mods,
            gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK
        );
        assert_eq!(parsed[0].act, Act::Action("win.split-right".into()));
        assert_eq!(
            parsed[0].accelerator().as_deref(),
            Some("<Shift><Control>t")
        );
        assert_eq!(parsed[1].key, gdk::Key::Home);
        assert_eq!(parsed[1].act, Act::Esc("\x1bOH".into()));
        assert!(parsed[1].matches(
            gdk::Key::Home,
            gdk::ModifierType::empty(),
            Mode::APP_CURSOR
        ));
        assert!(!parsed[1].matches(
            gdk::Key::Home,
            gdk::ModifierType::empty(),
            Mode::APP_CURSOR | Mode::ALT_SCREEN
        ));
        assert_eq!(parsed[1].accelerator(), None);
        assert_eq!(parsed[2].act, Act::PassThrough);
        assert_eq!(parsed[3].act, Act::Action("win.select-tab(1)".into()));
        assert_eq!(parsed[4].act, Act::Run("code".into(), vec!["-n".into()]));
    }
}
