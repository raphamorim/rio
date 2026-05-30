//! Smart-selection configuration: built-in rule set plus user
//! overrides.
//!
//! At runtime the rule list is the merge of [`default_rules`] and the
//! user's `[[smart-selection.rules]]` entries: a user rule whose
//! `name` matches a default replaces it (or removes it when
//! `enabled = false`); a rule with a fresh name is appended. Bad user
//! regexes log a warning and are skipped, so a typo in one rule
//! doesn't take the rest down.

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::crosswords::smart_select::SmartRule;

/// Precision constants. Higher wins; ties broken by match length.
/// Using `u8` keeps comparisons integer-only and reserves room for
/// future user rules without exhausting the space.
pub const PRECISION_URL: u8 = 200;
pub const PRECISION_HIGH: u8 = 150;
pub const PRECISION_MEDIUM: u8 = 130;
pub const PRECISION_LOW: u8 = 50;

/// `(name, pattern, precision)` for every built-in rule.
///
/// Patterns use the subset of regex syntax supported by
/// `regex_automata::hybrid::dfa` — notably, no look-around and ASCII
/// word boundaries only (`(?-u:\b)`; Unicode `\b` can't be compiled
/// to a DFA). The `url` rule allows `://` and `:/` to accommodate
/// `file:/path` forms.
pub fn default_rules() -> Vec<(&'static str, &'static str, u8)> {
    vec![
        (
            "url",
            // Brackets and parens are excluded so a URL inside `[…]`
            // or `(…)` stops at the boundary; producers that need a
            // bracketed URL preserved should emit OSC 8 (handled by
            // the fast path).
            r#"(?:https?|ftp|file|ssh|git|mailto|gemini|gopher|news|magnet|ipfs|ipns)://?[^\s<>"\\{}\^\x00-\x1f\[\]()]+"#,
            PRECISION_URL,
        ),
        (
            "file_line",
            r"[\w./~-]+\.[\w]+:\d+(?::\d+)?",
            PRECISION_HIGH,
        ),
        (
            "uuid",
            r"(?-u:\b)[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}(?-u:\b)",
            PRECISION_MEDIUM + 10,
        ),
        (
            "ipv4",
            r"(?-u:\b)(?:\d{1,3}\.){3}\d{1,3}(?-u:\b)",
            PRECISION_MEDIUM,
        ),
        (
            "email",
            r"(?-u:\b)[\w.+-]+@[\w.-]+\.[a-zA-Z]{2,}(?-u:\b)",
            PRECISION_MEDIUM,
        ),
        ("git_sha", r"(?-u:\b)[0-9a-f]{7,40}(?-u:\b)", PRECISION_LOW),
    ]
}

/// Compile the built-in rule set with no user customization. Panics
/// on a regex compile error because every default pattern is
/// hardcoded — a failure here means a developer typo, and `cargo
/// test` catches it before shipping.
pub fn compile_default_rules() -> Vec<SmartRule> {
    default_rules()
        .into_iter()
        .map(|(name, pattern, precision)| {
            SmartRule::new(name, pattern, precision).unwrap_or_else(|e| {
                panic!("invalid built-in smart-selection rule {name}: {e}")
            })
        })
        .collect()
}

/// `[smart-selection]` config table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartSelectionConfig {
    /// Master switch. When `false`, double-click falls straight to
    /// the existing semantic selection (the OSC 8 fast path is also
    /// bypassed because the runtime rule list is empty).
    #[serde(default = "default_bool_true")]
    pub enabled: bool,

    /// User-defined rules. Layered on top of the built-in defaults
    /// by name.
    #[serde(default)]
    pub rules: Vec<UserRule>,
}

impl Default for SmartSelectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rules: Vec::new(),
        }
    }
}

/// One entry in `[[smart-selection.rules]]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserRule {
    /// Identifier; matches against a built-in rule for override/removal.
    pub name: String,

    /// Regex pattern. Optional when overriding a default and only
    /// changing the precision, or when disabling a default.
    #[serde(default)]
    pub regex: Option<String>,

    /// Match precision (higher wins). Optional under the same
    /// conditions as `regex`.
    #[serde(default)]
    pub precision: Option<u8>,

    /// When `false`, removes the same-named default from the runtime
    /// rule list. Defaults to `true`.
    #[serde(default = "default_bool_true")]
    pub enabled: bool,
}

impl SmartSelectionConfig {
    /// Build the runtime rule list. Defaults come first; user rules
    /// either override (same `name`), remove (`enabled = false`), or
    /// append. Bad user regexes log a warning and the rule is
    /// skipped so the remaining rules still apply.
    pub fn compile(&self) -> Vec<SmartRule> {
        if !self.enabled {
            return Vec::new();
        }

        let mut merged: Vec<(String, String, u8)> = default_rules()
            .into_iter()
            .map(|(n, r, p)| (n.to_string(), r.to_string(), p))
            .collect();

        for user in &self.rules {
            if let Some(idx) = merged.iter().position(|(n, _, _)| n == &user.name) {
                if !user.enabled {
                    merged.remove(idx);
                    continue;
                }
                if let Some(regex) = &user.regex {
                    merged[idx].1 = regex.clone();
                }
                if let Some(precision) = user.precision {
                    merged[idx].2 = precision;
                }
            } else {
                if !user.enabled {
                    continue;
                }
                match (&user.regex, user.precision) {
                    (Some(regex), Some(precision)) => {
                        merged.push((user.name.clone(), regex.clone(), precision));
                    }
                    _ => {
                        warn!(
                            "smart-selection rule {:?} needs both `regex` and `precision`; skipping",
                            user.name,
                        );
                    }
                }
            }
        }

        merged
            .into_iter()
            .filter_map(|(name, pattern, precision)| {
                match SmartRule::new(name.clone(), &pattern, precision) {
                    Ok(rule) => Some(rule),
                    Err(e) => {
                        warn!("smart-selection rule {:?} failed to compile: {}", name, e);
                        None
                    }
                }
            })
            .collect()
    }
}

fn default_bool_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_default_rules_compile() {
        let rules = compile_default_rules();
        assert_eq!(rules.len(), default_rules().len());
    }

    #[test]
    fn default_config_compiles_to_default_rules() {
        let cfg = SmartSelectionConfig::default();
        assert_eq!(cfg.compile().len(), default_rules().len());
    }

    #[test]
    fn disabled_master_switch_yields_empty() {
        let cfg = SmartSelectionConfig {
            enabled: false,
            rules: vec![],
        };
        assert!(cfg.compile().is_empty());
    }

    #[test]
    fn user_rule_appended_when_name_is_fresh() {
        let cfg = SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "jira".into(),
                regex: Some(r"[A-Z]{2,10}-\d+".into()),
                precision: Some(100),
                enabled: true,
            }],
        };
        let compiled = cfg.compile();
        assert_eq!(compiled.len(), default_rules().len() + 1);
        assert_eq!(compiled.last().unwrap().name, "jira");
    }

    #[test]
    fn user_rule_disables_default_by_name() {
        let cfg = SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "git_sha".into(),
                regex: None,
                precision: None,
                enabled: false,
            }],
        };
        let compiled = cfg.compile();
        assert_eq!(compiled.len(), default_rules().len() - 1);
        assert!(compiled.iter().all(|r| r.name != "git_sha"));
    }

    #[test]
    fn user_rule_overrides_default_pattern_and_precision() {
        let cfg = SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "git_sha".into(),
                regex: Some(r"(?-u:\b)[0-9a-f]{12,40}(?-u:\b)".into()),
                precision: Some(60),
                enabled: true,
            }],
        };
        let compiled = cfg.compile();
        assert_eq!(compiled.len(), default_rules().len());
        let sha = compiled.iter().find(|r| r.name == "git_sha").unwrap();
        assert_eq!(sha.precision, 60);
    }

    #[test]
    fn user_rule_can_change_only_precision() {
        let cfg = SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "git_sha".into(),
                regex: None,
                precision: Some(60),
                enabled: true,
            }],
        };
        let sha = cfg
            .compile()
            .into_iter()
            .find(|r| r.name == "git_sha")
            .unwrap();
        assert_eq!(sha.precision, 60);
    }

    #[test]
    fn bad_user_regex_is_skipped_others_still_compile() {
        let cfg = SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "broken".into(),
                regex: Some(r"[unterminated".into()),
                precision: Some(10),
                enabled: true,
            }],
        };
        let compiled = cfg.compile();
        // Defaults still present; broken rule dropped.
        assert_eq!(compiled.len(), default_rules().len());
        assert!(compiled.iter().all(|r| r.name != "broken"));
    }

    #[test]
    fn user_rule_without_regex_or_precision_is_skipped() {
        let cfg = SmartSelectionConfig {
            enabled: true,
            rules: vec![UserRule {
                name: "incomplete".into(),
                regex: None,
                precision: None,
                enabled: true,
            }],
        };
        let compiled = cfg.compile();
        assert_eq!(compiled.len(), default_rules().len());
    }

    #[test]
    fn parses_from_toml() {
        let toml_src = r#"
enabled = true

[[rules]]
name = "jira"
regex = "[A-Z]{2,10}-\\d+"
precision = 100

[[rules]]
name = "git_sha"
enabled = false
"#;
        let cfg: SmartSelectionConfig = toml::from_str(toml_src).unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.rules.len(), 2);
        let compiled = cfg.compile();
        // +1 for jira, -1 for git_sha disabled.
        assert_eq!(compiled.len(), default_rules().len());
        assert!(compiled.iter().any(|r| r.name == "jira"));
        assert!(compiled.iter().all(|r| r.name != "git_sha"));
    }

    #[test]
    fn parses_inside_full_config() {
        use crate::config::Config;
        let src = r#"
[smart-selection]
enabled = true

[[smart-selection.rules]]
name = "ticket"
regex = "T-\\d+"
precision = 80
"#;
        let cfg: Config = toml::from_str(src).unwrap();
        assert!(cfg.smart_selection.enabled);
        assert_eq!(cfg.smart_selection.rules.len(), 1);
    }
}
