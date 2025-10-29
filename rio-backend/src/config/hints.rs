use serde::{Deserialize, Serialize};

/// Default alphabet for hint labels
pub const DEFAULT_HINTS_ALPHABET: &str = "jfkdls;ahgurieowpq";

/// Default URL regex pattern (same as Alacritty)
pub const DEFAULT_URL_REGEX: &str = "(ipfs:|ipns:|magnet:|mailto:|gemini://|gopher://|https://|http://|news:|file:|git://|ssh:|ftp://)[^\u{0000}-\u{001F}\u{007F}-\u{009F}<>\"\\s{-}\\^⟨⟩`\\\\]+";

/// Hints configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hints {
    /// Characters used for hint labels
    #[serde(default = "default_hints_alphabet")]
    pub alphabet: String,

    /// List of hint rules
    #[serde(default = "default_hints_enabled")]
    pub rules: Vec<Hint>,
}

impl Default for Hints {
    fn default() -> Self {
        Self {
            alphabet: default_hints_alphabet(),
            rules: default_hints_enabled(),
        }
    }
}

/// Individual hint configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hint {
    /// Regex pattern to match
    #[serde(default)]
    pub regex: Option<String>,

    /// Whether to include OSC 8 hyperlinks
    #[serde(default = "default_bool_false")]
    pub hyperlinks: bool,

    /// Whether to apply post-processing to matches
    #[serde(default = "default_bool_true", rename = "post-processing")]
    pub post_processing: bool,

    /// Whether hints persist after selection
    #[serde(default = "default_bool_false")]
    pub persist: bool,

    /// Action to perform when hint is activated
    #[serde(flatten)]
    pub action: HintAction,

    /// Mouse configuration for this hint
    #[serde(default)]
    pub mouse: HintMouse,

    /// Keyboard binding to activate hint mode
    #[serde(default)]
    pub binding: Option<HintBinding>,
}

/// Actions that can be performed with hints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HintAction {
    /// Built-in action
    Action { action: HintInternalAction },
    /// Custom command
    Command { command: HintCommand },
}

/// Built-in hint actions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum HintInternalAction {
    /// Copy the hint text to clipboard
    Copy,
    /// Paste the hint text
    Paste,
    /// Select the hint text
    Select,
    /// Move vi mode cursor to hint
    MoveViModeCursor,
}

/// Custom command configuration
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HintCommand {
    /// Simple command string
    Simple(String),
    /// Command with arguments
    WithArgs {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

/// Mouse configuration for hints
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HintMouse {
    /// Whether mouse highlighting is enabled
    #[serde(default = "default_bool_true")]
    pub enabled: bool,

    /// Required modifiers for mouse highlighting
    #[serde(default)]
    pub mods: Vec<String>,
}

impl Default for HintMouse {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        let default_mods = vec!["Super".to_string()];

        #[cfg(not(target_os = "macos"))]
        let default_mods = vec!["Alt".to_string()];

        Self {
            enabled: true,
            mods: default_mods,
        }
    }
}

/// Keyboard binding for hint activation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HintBinding {
    /// Key to press
    pub key: String,

    /// Required modifiers
    #[serde(default)]
    pub mods: Vec<String>,

    /// Terminal mode requirements
    #[serde(default)]
    pub mode: Vec<String>,
}

// Default functions
fn default_hints_alphabet() -> String {
    DEFAULT_HINTS_ALPHABET.to_string()
}

fn default_hints_enabled() -> Vec<Hint> {
    vec![Hint {
        regex: Some(DEFAULT_URL_REGEX.to_string()),
        hyperlinks: true,
        post_processing: true,
        persist: false,
        action: HintAction::Command {
            command: default_url_command(),
        },
        mouse: HintMouse::default(),
        binding: Some(HintBinding {
            key: "O".to_string(),
            mods: vec!["Control".to_string(), "Shift".to_string()],
            mode: Vec::new(),
        }),
    }]
}

fn default_url_command() -> HintCommand {
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return HintCommand::Simple("xdg-open".to_string());

    #[cfg(target_os = "macos")]
    return HintCommand::Simple("open".to_string());

    #[cfg(target_os = "windows")]
    return HintCommand::WithArgs {
        program: "cmd".to_string(),
        args: vec!["/c".to_string(), "start".to_string(), "".to_string()],
    };
}

fn default_bool_true() -> bool {
    true
}

fn default_bool_false() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hints_default() {
        let hints = Hints::default();
        assert_eq!(hints.alphabet, DEFAULT_HINTS_ALPHABET);
        assert_eq!(hints.rules.len(), 1);

        let default_hint = &hints.rules[0];
        assert!(default_hint.regex.is_some());
        assert!(default_hint.hyperlinks);
        assert!(default_hint.post_processing);
        assert!(!default_hint.persist);
    }

    #[test]
    fn test_hint_serialization() {
        let hint = Hint {
            regex: Some("test.*pattern".to_string()),
            hyperlinks: false,
            post_processing: true,
            persist: false,
            action: HintAction::Action {
                action: HintInternalAction::Copy,
            },
            mouse: HintMouse::default(),
            binding: None,
        };

        let serialized = toml::to_string(&hint).unwrap();
        let deserialized: Hint = toml::from_str(&serialized).unwrap();
        assert_eq!(hint, deserialized);
    }

    #[test]
    fn test_config_with_hints() {
        use crate::config::Config;

        let config_toml = r#"
[hints]
alphabet = "abcdef"

[[hints.rules]]
regex = "test.*pattern"
hyperlinks = false
post-processing = true
persist = false

[hints.rules.action]
action = "Copy"

[hints.rules.binding]
key = "T"
mods = ["Control"]
"#;

        let config: Config = toml::from_str(config_toml).unwrap();
        assert_eq!(config.hints.alphabet, "abcdef");
        assert_eq!(config.hints.rules.len(), 1);

        let hint = &config.hints.rules[0];
        assert_eq!(hint.regex, Some("test.*pattern".to_string()));
        assert!(!hint.hyperlinks);
        assert!(hint.post_processing);
        assert!(!hint.persist);
    }
}
