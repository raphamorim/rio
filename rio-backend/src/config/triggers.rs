use crate::config::colors::{deserialize_to_arr, ColorArray};
use serde::{Deserialize, Serialize};

/// Triggers configuration, loaded from a dedicated `triggers.toml` so the
/// file's top level is the rule list (`[[rules]]`).
///
/// Scanning scope: rules run against the FOCUSED pane's output as it
/// renders. A pane in a background tab is not scanned until it is
/// focused again, so a notify rule fires when you switch to that tab,
/// not at the moment the text appeared.
///
/// Substituted captures (`\1`...) in `run`/`coprocess` args come from
/// untrusted terminal output; no shell is involved and words are never
/// re-split, but a capture can begin with `-`. Put `--` before capture
/// arguments when the program supports it, e.g.
/// `run = { program = "notify-send", args = ["--", "\1"] }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Triggers {
    #[serde(default)]
    pub rules: Vec<Trigger>,
}

/// A single trigger rule: a regex matched against terminal output and the
/// action to perform on a match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trigger {
    /// Oniguruma regex matched against a line of terminal output.
    pub regex: String,

    /// Fire mid-line on every output batch instead of only when the line is
    /// finalized by a newline / cursor move.
    #[serde(default)]
    pub instant: bool,

    /// Fire at most once until the config is reloaded. Lets a rule drive one
    /// step of a sequence (e.g. a login probe) without re-firing when the
    /// same prompt reappears.
    #[serde(default)]
    pub once: bool,

    /// The action to perform. Externally tagged so the action's key names
    /// it, mirroring the `[hints]` config style.
    pub action: TriggerAction,
}

/// Desktop-notification urgency. `critical` banners even while rio is the
/// focused window — GNOME suppresses normal banners from the focused app and
/// only files them in the notification list — at the cost of GNOME keeping
/// critical notifications on screen until dismissed.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    /// freedesktop `urgency` hint level.
    pub fn level(self) -> u8 {
        match self {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        }
    }
}

/// `\0..\9` in textual parameters expand to the whole match and capture
/// groups, respectively.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerAction {
    /// Post a desktop notification.
    Notify {
        title: String,
        #[serde(default)]
        body: String,
        #[serde(default)]
        urgency: Urgency,
    },

    /// Set the tab color.
    TabColor {
        #[serde(deserialize_with = "deserialize_to_arr")]
        color: ColorArray,
    },

    /// Spawn a detached command (output discarded).
    Run {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },

    /// Highlight the matched text.
    Highlight {
        #[serde(deserialize_with = "deserialize_to_arr")]
        color: ColorArray,
    },

    /// Write text into the matched session's PTY.
    SendText { text: String },

    /// Run a command and write its stdout into the matched session's PTY.
    Coprocess {
        program: String,
        #[serde(default)]
        args: Vec<String>,
        /// Pipe the visible screen to the command's stdin. Lets the command
        /// see multi-line context (e.g. a wrapped block) that a single
        /// per-line capture group can't carry.
        #[serde(default)]
        feed_screen: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `triggers.toml` puts the rule list at the file's top level (`[[rules]]`,
    /// loaded as `Triggers` directly), so the tests parse the same shape rather
    /// than a `[[triggers.rules]]` wrapper that never appears in the real file.
    fn parse(content: &str) -> Triggers {
        toml::from_str::<Triggers>(content).unwrap()
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(parse("").rules.len(), 0);
    }

    #[test]
    fn parses_each_action() {
        let triggers = parse(
            r##"
            [[rules]]
            regex = "error: (.*)"
            [rules.action]
            notify = { title = "Error", body = "\\1", urgency = "critical" }

            [[rules]]
            regex = "(?i)warn"
            instant = true
            [rules.action]
            highlight = { color = "#FFAA00" }

            [[rules]]
            regex = "done"
            [rules.action]
            tab_color = { color = "#00FF00" }

            [[rules]]
            regex = "deploy"
            [rules.action]
            run = { program = "notify-send", args = ["deploy"] }

            [[rules]]
            regex = "password:"
            once = true
            [rules.action]
            send_text = { text = "hunter2\n" }

            [[rules]]
            regex = "now"
            [rules.action]
            coprocess = { program = "date" }
        "##,
        );

        assert_eq!(triggers.rules.len(), 6);
        assert!(matches!(
            triggers.rules[0].action,
            TriggerAction::Notify {
                urgency: Urgency::Critical,
                ..
            }
        ));
        assert!(triggers.rules[1].instant);
        assert!(matches!(
            triggers.rules[1].action,
            TriggerAction::Highlight { .. }
        ));
        assert!(triggers.rules[4].once);
        assert!(matches!(
            triggers.rules[5].action,
            TriggerAction::Coprocess { .. }
        ));
    }

    #[test]
    fn regex_compiles() {
        for rule in parse(
            r#"
            [[rules]]
            regex = "error: (.*)"
            [rules.action]
            notify = { title = "e", body = "\\1" }
        "#,
        )
        .rules
        {
            onig::Regex::new(&rule.regex).expect("regex compiles");
        }
    }
}
