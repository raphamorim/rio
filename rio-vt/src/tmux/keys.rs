//! The prefix key.
//!
//! A control client's input is commands, so tmux never sees keystrokes from
//! it and its key tables don't run: `send-keys` goes straight to the pane.
//! The frontend plays that part instead, working from the bindings tmux
//! reports. Prompts are for the frontend to ask, see [`super::prompt`]. Other
//! bindings only run if they make sense without a tmux display: menus and
//! modes draw on a terminal a control client doesn't have.

use super::prompt::TmuxPrompt;
use crate::time::Instant;
use rustc_hash::FxHashMap;
use std::time::Duration;

/// Asks for the prefixes, repeat time and tmux version, in the order
/// `parse_options` reads them. Formats follow session options the way the
/// keys themselves do.
pub const KEY_OPTIONS: &str =
    "display-message -p '#{prefix} #{prefix2} #{repeat-time} #{version}'";

pub const LIST_PREFIX_KEYS: &str = "list-keys -T prefix";

/// Draw on the client's terminal or take over its input, which a control
/// client can't show: `copy-mode` would silently eat all further input.
const NEEDS_DISPLAY: &[&str] = &[
    "choose-buffer",
    "choose-client",
    "choose-tree",
    "clock-mode",
    "command-prompt",
    "confirm-before",
    "copy-mode",
    "customize-mode",
    "display-menu",
    "display-panes",
    "display-popup",
    "find-window",
    "lock-client",
    "lock-server",
    "lock-session",
    "suspend-client",
];

/// Short names for [`NEEDS_DISPLAY`], which can be typed at a prompt.
const NEEDS_DISPLAY_ALIASES: &[(&str, &str)] = &[
    ("confirm", "confirm-before"),
    ("displayp", "display-panes"),
    ("findw", "find-window"),
    ("lock", "lock-server"),
    ("lockc", "lock-client"),
    ("locks", "lock-session"),
    ("menu", "display-menu"),
    ("popup", "display-popup"),
    ("suspendc", "suspend-client"),
];

/// Nothing but something to read. Harmless next to other commands, as in
/// `source-file ... ; display-message Reloaded`, but pointless on their own.
const OUTPUT_ONLY: &[&str] = &[
    "display-message",
    "list-buffers",
    "list-keys",
    "show-messages",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    /// Stays available for `repeat-time` after running, as `bind -r` asks.
    repeat: bool,
    action: BindingAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum BindingAction {
    /// One line of commands, ready to send.
    Run(String),
    /// Something to ask first.
    Prompt(Box<TmuxPrompt>),
    /// The name of the command that can't work here.
    Unsupported(String),
}

/// What the prefix key does in a session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TmuxKeys {
    prefix: Option<String>,
    prefix2: Option<String>,
    repeat_time: Duration,
    bindings: FxHashMap<String, Binding>,
    before_3_3: bool,
}

impl Default for TmuxKeys {
    fn default() -> Self {
        Self {
            prefix: None,
            prefix2: None,
            repeat_time: Duration::from_millis(500),
            bindings: FxHashMap::default(),
            before_3_3: false,
        }
    }
}

/// Where the keyboard stands between key presses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrefixState {
    armed: bool,
    repeat_until: Option<Instant>,
}

impl PrefixState {
    /// The prefix was pressed and the next key is for tmux.
    pub fn is_armed(&self) -> bool {
        self.armed
    }
}

/// What to do with a key press.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyPress {
    /// Not for tmux: treat it as any other key.
    Pass,
    /// Taken, with nothing further to do.
    Consumed,
    /// Taken: run these commands with `ControlSession::run_binding`.
    Run(String),
    /// Taken: ask this, then run the outcome with
    /// `ControlSession::run_prompt`.
    Prompt(Box<TmuxPrompt>),
    /// Taken, but bound to a command that can't work on a control client.
    Unsupported(String),
}

impl TmuxKeys {
    /// Read a [`KEY_OPTIONS`] reply.
    pub fn parse_options(&mut self, line: &str) {
        let mut fields = line.split(' ');
        let mut key = || {
            fields
                .next()
                .filter(|key| !key.is_empty() && *key != "None")
                .map(str::to_string)
        };
        self.prefix = key();
        self.prefix2 = key();
        if let Some(time) = fields.next().and_then(|time| time.parse().ok()) {
            self.repeat_time = Duration::from_millis(time);
        }
        self.before_3_3 = fields.next().is_some_and(is_before_3_3);
    }

    /// Whether tmux is known to be older than 3.3.
    pub fn before_3_3(&self) -> bool {
        self.before_3_3
    }

    /// Read a [`LIST_PREFIX_KEYS`] reply.
    pub fn parse_bindings<'a>(&mut self, lines: impl Iterator<Item = &'a str>) {
        self.bindings = lines.filter_map(parse_binding).collect();
    }

    /// The prefix as tmux names it, for showing the user.
    pub fn prefix(&self) -> Option<&str> {
        self.prefix.as_deref()
    }

    /// Work out what a press of `key`, a tmux key name, means. `held` is for
    /// the key repeat of a key being held down.
    pub fn press(
        &self,
        state: &mut PrefixState,
        key: &str,
        held: bool,
        now: Instant,
    ) -> KeyPress {
        if state.armed {
            // Holding the prefix a moment too long is not pressing it twice.
            if held {
                return KeyPress::Consumed;
            }
            state.armed = false;
            return match self.bindings.get(key) {
                Some(binding) => self.fire(state, binding, now),
                // As in tmux, a key with no binding just ends the prefix.
                None => KeyPress::Consumed,
            };
        }

        if let Some(until) = state.repeat_until.take() {
            if now < until {
                if let Some(binding) =
                    self.bindings.get(key).filter(|binding| binding.repeat)
                {
                    return self.fire(state, binding, now);
                }
            }
        }

        if held {
            return KeyPress::Pass;
        }
        if self.prefix.as_deref() == Some(key) || self.prefix2.as_deref() == Some(key) {
            state.armed = true;
            return KeyPress::Consumed;
        }
        KeyPress::Pass
    }

    fn fire(&self, state: &mut PrefixState, binding: &Binding, now: Instant) -> KeyPress {
        match &binding.action {
            BindingAction::Run(commands) => {
                if binding.repeat {
                    state.repeat_until = Some(now + self.repeat_time);
                }
                KeyPress::Run(commands.clone())
            }
            BindingAction::Prompt(prompt) => KeyPress::Prompt(prompt.clone()),
            BindingAction::Unsupported(command) => KeyPress::Unsupported(command.clone()),
        }
    }
}

/// Read a version like `3.2a`. Builds like `next-3.7` count as recent.
fn is_before_3_3(version: &str) -> bool {
    let mut parts = version.split('.');
    let major = parts.next().and_then(|major| major.parse::<u32>().ok());
    let minor = parts.next().and_then(|minor| {
        let digits = minor.find(|c: char| !c.is_ascii_digit());
        minor[..digits.unwrap_or(minor.len())].parse::<u32>().ok()
    });
    matches!((major, minor), (Some(major), Some(minor)) if (major, minor) < (3, 3))
}

fn parse_binding(line: &str) -> Option<(String, Binding)> {
    let (word, mut rest) = next_token(line)?;
    if word != "bind-key" {
        return None;
    }

    let mut repeat = false;
    let key = loop {
        let (token, after) = next_token(rest)?;
        rest = after;
        match token {
            "-r" => repeat = true,
            "-T" | "-N" => rest = next_token(rest)?.1,
            key => break key,
        }
    };
    // Listed the way it would be typed in a config file.
    let key = match key.strip_prefix('\\') {
        Some(key) if !key.is_empty() => key,
        _ => key,
    };

    let commands = split_commands(rest.trim());
    let action = classify(&commands)?;
    Some((key.to_string(), Binding { repeat, action }))
}

fn classify(commands: &[&str]) -> Option<BindingAction> {
    let names: Vec<&str> = commands
        .iter()
        .filter_map(|command| command.split_whitespace().next())
        .collect();

    if let ([command], ["command-prompt" | "confirm-before"]) = (commands, &names[..]) {
        return Some(match TmuxPrompt::parse(command) {
            Ok(prompt) => BindingAction::Prompt(Box::new(prompt)),
            Err(name) => BindingAction::Unsupported(name),
        });
    }
    if let Some(name) = names.iter().find(|name| NEEDS_DISPLAY.contains(name)) {
        return Some(BindingAction::Unsupported(name.to_string()));
    }
    match names.as_slice() {
        [] => None,
        [name] if OUTPUT_ONLY.contains(name) => {
            Some(BindingAction::Unsupported(name.to_string()))
        }
        _ => Some(BindingAction::Run(commands.join(" ; "))),
    }
}

/// The first command in a line of commands that can't work on a control
/// client, the way they would be typed at a prompt.
pub fn unsupported_command(line: &str) -> Option<String> {
    let mut rest = line;
    loop {
        let end = scan(rest, |index, byte, _| (byte == b';').then_some(index));
        let command = &rest[..end.unwrap_or(rest.len())];
        let name = command.split_whitespace().next().and_then(needs_display);
        if let Some(name) = name {
            return Some(name.to_string());
        }
        rest = &rest[end? + 1..];
    }
}

/// The command `name` stands for, if it needs a display. tmux also takes
/// the start of a name, as long as it's the only command starting that way;
/// four letters is enough to tell these apart from what can run, and
/// `display` is an alias of `display-message`.
fn needs_display(name: &str) -> Option<&'static str> {
    let exact = NEEDS_DISPLAY.iter().find(|command| **command == name);
    let alias = NEEDS_DISPLAY_ALIASES
        .iter()
        .find(|(alias, _)| *alias == name)
        .map(|(_, command)| command);
    let start = NEEDS_DISPLAY.iter().find(|command| {
        name.len() >= 4 && name != "display" && command.starts_with(name)
    });
    exact.or(alias).or(start).copied()
}

/// The next whitespace separated token, honouring quotes.
pub(super) fn next_token(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    let end = scan(text, |index, byte, _| {
        byte.is_ascii_whitespace().then_some(index)
    })
    .unwrap_or(text.len());
    Some((&text[..end], &text[end..]))
}

/// Split a listed binding into its commands.
///
/// `list-keys` separates them with `\;`, but that's only right as an argument
/// to `bind-key`: sent as a command it would reach the first command as an
/// argument of its own.
fn split_commands(text: &str) -> Vec<&str> {
    let mut commands = Vec::new();
    let mut rest = text;
    while let Some(index) = scan(rest, |index, byte, bytes| {
        let lone = |index: usize| {
            bytes
                .get(index)
                .is_none_or(|byte: &u8| byte.is_ascii_whitespace())
        };
        (byte == b'\\'
            && bytes.get(index + 1) == Some(&b';')
            && (index == 0 || bytes[index - 1].is_ascii_whitespace())
            && lone(index + 2))
        .then_some(index)
    }) {
        commands.push(rest[..index].trim());
        rest = &rest[index + 2..];
    }
    commands.push(rest.trim());
    commands.retain(|command| !command.is_empty());
    commands
}

/// Walk `text` outside of quotes until `found` gives an answer. Backslash
/// sequences are offered to `found` before being skipped.
fn scan(text: &str, found: impl Fn(usize, u8, &[u8]) -> Option<usize>) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(b'"') if byte == b'\\' => index += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => (),
            None => {
                if let Some(answer) = found(index, byte, bytes) {
                    return Some(answer);
                }
                match byte {
                    b'\\' => index += 1,
                    b'\'' | b'"' => quote = Some(byte),
                    _ => (),
                }
            }
        }
        index += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_compared_by_number() {
        for version in ["3.2a", "3.2", "2.9a", "1.8"] {
            assert!(is_before_3_3(version), "{version}");
        }
        for version in ["3.3", "3.3a", "3.4", "3.6", "3.10", "4.0", "next-3.7", ""] {
            assert!(!is_before_3_3(version), "{version}");
        }
    }

    #[test]
    fn the_version_comes_with_the_options() {
        let mut keys = TmuxKeys::default();
        keys.parse_options("C-b None 500 3.2a");
        assert!(keys.before_3_3());
        keys.parse_options("C-b None 500 3.6");
        assert!(!keys.before_3_3());
        keys.parse_options("C-b None 500");
        assert!(!keys.before_3_3());
    }

    #[test]
    fn prompts_are_for_the_frontend_to_ask() {
        let mut keys = TmuxKeys::default();
        keys.parse_options("C-b None 500");
        keys.parse_bindings(
            [
                r#"bind-key -T prefix : command-prompt"#,
                r##"bind-key -T prefix x confirm-before -p "kill-pane #P? (y/n)" kill-pane"##,
                r#"bind-key -T prefix f command-prompt "find-window -Z -- '%%'""#,
                r#"bind-key -T prefix m command-prompt \; display-message hello"#,
            ]
            .into_iter(),
        );
        let press = |key| {
            let mut state = PrefixState::default();
            keys.press(&mut state, "C-b", false, Instant::now());
            keys.press(&mut state, key, false, Instant::now())
        };

        assert!(matches!(press(":"), KeyPress::Prompt(_)));
        assert!(matches!(press("x"), KeyPress::Prompt(_)));
        assert_eq!(
            press("f"),
            KeyPress::Unsupported(String::from("find-window"))
        );
        assert_eq!(
            press("m"),
            KeyPress::Unsupported(String::from("command-prompt"))
        );
    }

    #[test]
    fn typed_commands_are_checked_one_by_one() {
        assert_eq!(unsupported_command("split-window -h"), None);
        assert_eq!(unsupported_command(""), None);
        assert_eq!(unsupported_command("display hello"), None);
        assert_eq!(unsupported_command("neww ; splitw"), None);
        assert_eq!(
            unsupported_command("neww ; copy-mode"),
            Some(String::from("copy-mode"))
        );
        assert_eq!(
            unsupported_command("neww;copy-mode"),
            Some(String::from("copy-mode"))
        );
        assert_eq!(
            unsupported_command("popup -E top"),
            Some(String::from("display-popup"))
        );
        assert_eq!(unsupported_command("copy"), Some(String::from("copy-mode")));
    }

    #[test]
    fn quoted_and_escaped_separators_are_not_commands() {
        assert_eq!(unsupported_command("rename-window 'a ; copy-mode'"), None);
        assert_eq!(unsupported_command(r"bind x neww \; copy-mode"), None);
    }

    /// Lines as real tmux 3.2a lists them.
    const LISTING: &[&str] = &[
        r##"bind-key    -T prefix C-b     send-prefix"##,
        r##"bind-key    -T prefix \"      split-window"##,
        r##"bind-key    -T prefix \#      list-buffers"##,
        r##"bind-key    -T prefix %       split-window -h"##,
        r##"bind-key    -T prefix &       confirm-before -p "kill-window #W? (y/n)" kill-window"##,
        r##"bind-key    -T prefix ,       command-prompt -I "#W" "rename-window -- '%%'""##,
        r##"bind-key    -T prefix \;      last-pane"##,
        r##"bind-key    -T prefix [       copy-mode"##,
        r##"bind-key -r -T prefix K       resize-pane -U 5"##,
        r##"bind-key -r -T prefix DC      refresh-client -c"##,
        r##"bind-key    -T prefix r       source-file /dev/null \; display-message Reloaded"##,
        r##"bind-key    -T prefix a       split-window -h \; select-pane -L"##,
        r##"bind-key    -T prefix w       set-option -g @x "a \; b""##,
        r##"bind-key    -T prefix v       if-shell -F "#{pane_in_mode}" "select-pane -L" "select-pane -R""##,
        r##"bind-key    -T prefix y       display-message "it's""##,
    ];

    fn keys() -> TmuxKeys {
        let mut keys = TmuxKeys::default();
        keys.parse_options("C-b C-Space 500");
        keys.parse_bindings(LISTING.iter().copied());
        keys
    }

    fn press(keys: &TmuxKeys, state: &mut PrefixState, key: &str) -> KeyPress {
        keys.press(state, key, false, Instant::now())
    }

    /// What pressing the prefix and then `key` does.
    fn chord(key: &str) -> KeyPress {
        let keys = keys();
        let mut state = PrefixState::default();
        assert_eq!(press(&keys, &mut state, "C-b"), KeyPress::Consumed);
        assert!(state.is_armed());
        let result = press(&keys, &mut state, key);
        assert!(!state.is_armed());
        result
    }

    fn run(commands: &str) -> KeyPress {
        KeyPress::Run(commands.to_string())
    }

    #[test]
    fn options_are_read() {
        let keys = keys();
        assert_eq!(keys.prefix(), Some("C-b"));
        assert_eq!(keys.prefix2.as_deref(), Some("C-Space"));

        let mut keys = TmuxKeys::default();
        keys.parse_options("C-a None 250");
        assert_eq!(keys.prefix(), Some("C-a"));
        assert_eq!(keys.prefix2, None);
        assert_eq!(keys.repeat_time, Duration::from_millis(250));
    }

    #[test]
    fn missing_options_disable_the_prefix() {
        let mut keys = TmuxKeys::default();
        keys.parse_options("");
        let mut state = PrefixState::default();
        assert_eq!(press(&keys, &mut state, "C-b"), KeyPress::Pass);
    }

    #[test]
    fn keys_pass_until_the_prefix_is_pressed() {
        let keys = keys();
        let mut state = PrefixState::default();
        assert_eq!(press(&keys, &mut state, "%"), KeyPress::Pass);
        assert!(!state.is_armed());
    }

    #[test]
    fn a_binding_runs_after_the_prefix() {
        assert_eq!(chord("%"), run("split-window -h"));
    }

    #[test]
    fn the_second_prefix_arms_too() {
        let keys = keys();
        let mut state = PrefixState::default();
        assert_eq!(press(&keys, &mut state, "C-Space"), KeyPress::Consumed);
        assert_eq!(press(&keys, &mut state, "%"), run("split-window -h"));
    }

    #[test]
    fn the_prefix_twice_reaches_the_pane() {
        assert_eq!(chord("C-b"), run("send-prefix"));
    }

    #[test]
    fn keys_listed_with_a_backslash_are_found() {
        assert_eq!(chord("\""), run("split-window"));
        assert_eq!(chord(";"), run("last-pane"));
    }

    #[test]
    fn an_unbound_key_ends_the_prefix() {
        let keys = keys();
        let mut state = PrefixState::default();
        press(&keys, &mut state, "C-b");
        assert_eq!(press(&keys, &mut state, "F7"), KeyPress::Consumed);
        assert_eq!(press(&keys, &mut state, "%"), KeyPress::Pass);
    }

    #[test]
    fn holding_the_prefix_does_not_press_it_twice() {
        let keys = keys();
        let mut state = PrefixState::default();
        let now = Instant::now();
        keys.press(&mut state, "C-b", false, now);
        assert_eq!(keys.press(&mut state, "C-b", true, now), KeyPress::Consumed);
        assert!(state.is_armed());
    }

    #[test]
    fn a_held_key_cannot_arm() {
        let keys = keys();
        let mut state = PrefixState::default();
        assert_eq!(
            keys.press(&mut state, "C-b", true, Instant::now()),
            KeyPress::Pass
        );
    }

    #[test]
    fn repeatable_bindings_stay_available() {
        let keys = keys();
        let mut state = PrefixState::default();
        let start = Instant::now();
        keys.press(&mut state, "C-b", false, start);
        let resize = run("resize-pane -U 5");
        assert_eq!(keys.press(&mut state, "K", false, start), resize);

        let soon = start + Duration::from_millis(400);
        assert_eq!(keys.press(&mut state, "K", false, soon), resize);
        assert_eq!(keys.press(&mut state, "K", true, soon), resize);

        // Each repeat extends the window.
        let later = soon + Duration::from_millis(400);
        assert_eq!(keys.press(&mut state, "K", false, later), resize);

        let too_late = later + Duration::from_millis(600);
        assert_eq!(keys.press(&mut state, "K", false, too_late), KeyPress::Pass);
    }

    #[test]
    fn another_key_leaves_the_repeat_window() {
        let keys = keys();
        let mut state = PrefixState::default();
        let now = Instant::now();
        keys.press(&mut state, "C-b", false, now);
        keys.press(&mut state, "K", false, now);
        assert_eq!(keys.press(&mut state, "x", false, now), KeyPress::Pass);
        assert_eq!(keys.press(&mut state, "K", false, now), KeyPress::Pass);
    }

    #[test]
    fn the_prefix_still_works_inside_the_repeat_window() {
        let keys = keys();
        let mut state = PrefixState::default();
        let now = Instant::now();
        keys.press(&mut state, "C-b", false, now);
        keys.press(&mut state, "K", false, now);
        assert_eq!(
            keys.press(&mut state, "C-b", false, now),
            KeyPress::Consumed
        );
        assert!(state.is_armed());
    }

    #[test]
    fn a_plain_binding_opens_no_repeat_window() {
        let keys = keys();
        let mut state = PrefixState::default();
        let now = Instant::now();
        keys.press(&mut state, "C-b", false, now);
        keys.press(&mut state, "%", false, now);
        assert_eq!(keys.press(&mut state, "%", false, now), KeyPress::Pass);
    }

    #[test]
    fn command_sequences_are_rejoined_to_run() {
        assert_eq!(chord("a"), run("split-window -h ; select-pane -L"));
    }

    #[test]
    fn output_is_fine_after_other_commands() {
        assert_eq!(
            chord("r"),
            run("source-file /dev/null ; display-message Reloaded")
        );
    }

    #[test]
    fn quoted_arguments_stay_whole() {
        assert_eq!(chord("w"), run(r#"set-option -g @x "a \; b""#));
        assert_eq!(
            chord("v"),
            run(r##"if-shell -F "#{pane_in_mode}" "select-pane -L" "select-pane -R""##)
        );
    }

    #[test]
    fn commands_needing_a_display_are_refused() {
        assert_eq!(chord("["), KeyPress::Unsupported("copy-mode".to_string()));
    }

    #[test]
    fn one_command_needing_a_display_refuses_the_lot() {
        let mut keys = keys();
        keys.parse_bindings(
            ["bind-key -T prefix c new-window \\; copy-mode"]
                .iter()
                .copied(),
        );
        let mut state = PrefixState::default();
        press(&keys, &mut state, "C-b");
        assert_eq!(
            press(&keys, &mut state, "c"),
            KeyPress::Unsupported("copy-mode".to_string())
        );
    }

    #[test]
    fn output_alone_is_refused() {
        assert_eq!(
            chord("#"),
            KeyPress::Unsupported("list-buffers".to_string())
        );
        assert_eq!(
            chord("y"),
            KeyPress::Unsupported("display-message".to_string())
        );
    }

    #[test]
    fn an_unsupported_repeat_binding_opens_no_window() {
        let mut keys = keys();
        keys.parse_bindings(["bind-key -r -T prefix c copy-mode"].iter().copied());
        let mut state = PrefixState::default();
        let now = Instant::now();
        keys.press(&mut state, "C-b", false, now);
        keys.press(&mut state, "c", false, now);
        assert_eq!(keys.press(&mut state, "c", false, now), KeyPress::Pass);
    }

    #[test]
    fn notes_are_skipped() {
        let (key, binding) =
            parse_binding(r#"bind-key -N "Split it" -T prefix % split-window -h"#)
                .unwrap();
        assert_eq!(key, "%");
        assert_eq!(
            binding.action,
            BindingAction::Run("split-window -h".to_string())
        );
    }

    #[test]
    fn other_lines_are_ignored() {
        assert_eq!(parse_binding(""), None);
        assert_eq!(parse_binding("something else"), None);
        assert_eq!(parse_binding("bind-key -T prefix x"), None);
    }

    #[test]
    fn a_backslash_key_survives() {
        let (key, _) = parse_binding(r"bind-key -T prefix \\ last-pane").unwrap();
        assert_eq!(key, "\\");
    }
}
