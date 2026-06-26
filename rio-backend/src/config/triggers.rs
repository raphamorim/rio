use crate::config::colors::{deserialize_to_arr, ColorArray};
use serde::{Deserialize, Serialize};

/// Triggers configuration, loaded from a dedicated `triggers.toml` so the
/// file's top level is the rule list (`[[rules]]`).
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
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Root {
        #[serde(default)]
        triggers: Triggers,
    }

    fn parse(content: &str) -> Triggers {
        toml::from_str::<Root>(content).unwrap().triggers
    }

    #[test]
    fn empty_is_default() {
        assert_eq!(parse("").rules.len(), 0);
    }

    #[test]
    fn parses_each_action() {
        let triggers = parse(
            r##"
            [[triggers.rules]]
            regex = "error: (.*)"
            [triggers.rules.action]
            notify = { title = "Error", body = "\\1", urgency = "critical" }

            [[triggers.rules]]
            regex = "(?i)warn"
            instant = true
            [triggers.rules.action]
            highlight = { color = "#FFAA00" }

            [[triggers.rules]]
            regex = "done"
            [triggers.rules.action]
            tab_color = { color = "#00FF00" }

            [[triggers.rules]]
            regex = "deploy"
            [triggers.rules.action]
            run = { program = "notify-send", args = ["deploy"] }

            [[triggers.rules]]
            regex = "password:"
            [triggers.rules.action]
            send_text = { text = "hunter2\n" }

            [[triggers.rules]]
            regex = "now"
            [triggers.rules.action]
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
        assert!(matches!(
            triggers.rules[5].action,
            TriggerAction::Coprocess { .. }
        ));
    }

    #[test]
    fn regex_compiles() {
        for rule in parse(
            r#"
            [[triggers.rules]]
            regex = "error: (.*)"
            [triggers.rules.action]
            notify = { title = "e", body = "\\1" }
        "#,
        )
        .rules
        {
            onig::Regex::new(&rule.regex).expect("regex compiles");
        }
    }
}
