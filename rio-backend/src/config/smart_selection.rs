//! Built-in smart-selection rules.
//!
//! Phase 2 ships a hardcoded rule set tuned to win for the common
//! double-click targets — URLs, file:line:col, UUIDs, IPv4 addresses,
//! emails, git SHAs. Phase 3 will add a user-facing config section
//! that layers on top of these defaults.

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
        ("file_line", r"[\w./~-]+\.[\w]+:\d+(?::\d+)?", PRECISION_HIGH),
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

/// Compile the built-in rule set. Panics on a regex compile error
/// because every default pattern is hardcoded — a failure here means a
/// developer typo, and `cargo test` will catch it before shipping.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_default_rules_compile() {
        let rules = compile_default_rules();
        assert_eq!(rules.len(), default_rules().len());
    }
}
