use serde::{Deserialize, Serialize};

/// Default alphabet for hint labels
pub const DEFAULT_HINTS_ALPHABET: &str = "jfkdls;ahgurieowpq";

/// Default URL/path regex pattern.
///
/// Ported verbatim from ghostty's `src/config/url.zig`. Requires a regex
/// engine with lookbehind support — rio uses oniguruma via the `onig`
/// crate. Three alternations:
///
/// 1. **Schemed URLs** — `http://`, `https://`, `mailto:`, `file:`, `ssh:`,
///    `magnet:`, `ipfs://`, `gemini://`, etc. IPv6 literals supported.
///    Trailing `.` / `,` and unbalanced parens are excluded via lookbehind.
/// 2. **Rooted or explicitly-relative paths** — `/abs`, `./rel`, `../rel`,
///    `~/x`, `.hidden/x`, `$VAR/x`. Each prefix is guarded by lookbehinds
///    so the `~/` inside `foo~/bar` and the `/` inside `foo/bar` aren't
///    mis-matched. Paths with internal spaces are supported when they
///    contain a dotted filename segment.
/// 3. **Bare relative paths** — `word/.../name.ext`. A dotted segment is
///    required, and lookbehinds prevent matching mid-word starts.
pub const DEFAULT_URL_REGEX: &str = concat!(
 // schemed URLs
    "(?:https?://|mailto:|ftp://|file:|ssh:|git://|ssh://|tel:|magnet:|ipfs://|ipns://|gemini://|gopher://|news:)",
    "(?:",
        r"(?:\[[:0-9a-fA-F]+(?:[:0-9a-fA-F]*)+\](?::[0-9]+)?)",
        "|",
        r"[\w\-.~:/?#@!$&*+,;=%]+(?:[\(\[]\w*[\)\]])?",
    ")+",
    r"(?<![,.])",
    "|",
 // rooted or explicitly-relative paths
    r"(?:\.\./|\./|(?<!\w)~/|(?:[\w][\w\-.]*/)*(?<!\w)\$[A-Za-z_]\w*/|\.[\w][\w\-.]*/|(?<![\w~/])/(?!/))",
    "(?:",
 // Dotted: file-like, allows internal spaces around dotted segments.
        r"(?=[\w\-.~:/?#@!$&*+;=%]*\.)",
        r"[\w\-.~:/?#@!$&*+;=%]+",
        r"(?:(?<!:) (?!\w+://)(?!\.{0,2}/)(?!~/)[\w\-.~:/?#@!$&*+;=%]*[/.])*",
        r"(?<!:)",
        r"(?: +(?= *$))?",
        "|",
 // Non-dotted: directory-like, broader.
        r"(?![\w\-.~:/?#@!$&*+;=%]*\.)",
        r"[\w\-.~:/?#@!$&*+;=%]+",
        r"(?:(?<!:) (?!\w+://)(?!\.{0,2}/)(?!~/)[\w\-.~:/?#@!$&*+;=%]+)*",
        r"(?<!:)",
        r"(?: +(?= *$))?",
    ")",
    "|",
 // bare relative paths (word/foo.ext)
    r"(?=[\w\-.~:/?#@!$&*+;=%]*\.)",
    r"(?<!\$\d*)(?<!\w)[\w][\w\-.]*/",
    r"[\w\-.~:/?#@!$&*+;=%]+",
    r"(?<!:)",
    r"(?: +(?= *$))?",
);

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
    fn test_default_regex_compiles() {
        onig::Regex::new(DEFAULT_URL_REGEX).expect("default regex must compile");
    }

    /// Given input text, return every leftmost non-overlapping match produced
    /// by `DEFAULT_URL_REGEX`. Used to verify the path branches.
    fn find_all(input: &str) -> Vec<&str> {
        let re = onig::Regex::new(DEFAULT_URL_REGEX).unwrap();
        re.find_iter(input).map(|(s, e)| &input[s..e]).collect()
    }

    #[test]
    fn test_default_regex_matches_schemed_urls() {
        assert_eq!(
            find_all("visit https://rioterm.com here"),
            vec!["https://rioterm.com"]
        );
        assert_eq!(find_all("file://foo"), vec!["file://foo"]);
    }

    #[test]
    fn test_default_regex_matches_rooted_paths() {
        // Dotted paths (file-like): match stops at the next non-dotted token.
        assert_eq!(find_all("open ~/notes.md please"), vec!["~/notes.md"],);
        assert_eq!(find_all("see ./script.sh"), vec!["./script.sh"]);
        assert_eq!(
            find_all("check ../parent/file.txt"),
            vec!["../parent/file.txt"],
        );

        // Non-dotted (directory-like): absorbs trailing spaces+words because
        // the path could be a directory whose name contains spaces (e.g.
        // `~/Desktop please/...`). This matches ghostty's behavior.
        assert_eq!(find_all("open ~/Desktop please"), vec!["~/Desktop please"]);
        assert_eq!(find_all("cd /tmp/foo"), vec!["/tmp/foo"]);
        assert_eq!(find_all("logs at $HOME/logs"), vec!["$HOME/logs"]);
    }

    #[test]
    fn test_default_regex_matches_bare_relative_paths_with_extension() {
        assert_eq!(find_all("edit src/main.rs now"), vec!["src/main.rs"]);
        assert_eq!(
            find_all("see frontends/rioterm/src/hints.rs"),
            vec!["frontends/rioterm/src/hints.rs"]
        );
    }

    #[test]
    fn test_default_regex_rejects_midword_slash() {
        // Lookbehind `(?<![\w~/])/` keeps the `/` inside `foo/bar` from
        // anchoring the rooted-path branch. Branch 3 also fails (no dot).
        assert!(find_all("foo/bar").is_empty());
    }

    #[test]
    fn test_default_regex_rejects_midword_tilde() {
        // Lookbehind `(?<!\w)~/` rejects the `~/bar` inside `foo~/bar`.
        assert!(find_all("foo~/bar").is_empty());
    }

    #[test]
    fn test_default_regex_strips_trailing_punctuation_on_urls() {
        // `(?<![,.])` excludes the trailing period.
        assert_eq!(
            find_all("see https://example.com."),
            vec!["https://example.com"],
        );
    }

    #[test]
    fn test_default_regex_matches_dot_prefixed_paths() {
        // `.config/foo.txt` matches the `.word/` branch (hidden dirs).
        assert_eq!(
            find_all(".config/rio/config.toml"),
            vec![".config/rio/config.toml"]
        );
    }

    #[test]
    fn test_default_regex_prefers_bare_relative_over_embedded_slash() {
        // `Compiling src/config/url.zig` — the bare-relative branch anchors
        // at `src/...` and wins over the rooted `/config/url.zig` because
        // it starts earlier in the text.
        assert_eq!(
            find_all("Compiling src/config/url.zig"),
            vec!["src/config/url.zig"],
        );
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
