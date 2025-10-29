use serde::{Deserialize, Serialize};

// Examples:
// { key = "w", mods: "super", action = "quit" }
// { key = "Home", mods: "super | shift", esc = "\x1b[5~" }

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    #[serde(default = "String::default")]
    pub with: String,
    #[serde(default = "String::default")]
    pub action: String,
    #[serde(default = "String::default")]
    pub esc: String,
    #[serde(default = "String::default")]
    pub mode: String,
}

pub type KeyBindings = Vec<KeyBinding>;

#[derive(Default, Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct Bindings {
    pub keys: KeyBindings,
}

#[cfg(test)]
mod tests {

    use crate::config::bindings::Bindings;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Root {
        #[serde(default = "Bindings::default")]
        bindings: Bindings,
    }

    #[test]
    fn test_valid_key_action() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Q', with = 'super', action = 'quit' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Q");
        assert_eq!(decoded.bindings.keys[0].with.to_owned(), "super");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), "quit");
        assert!(decoded.bindings.keys[0].esc.to_owned().is_empty());
    }

    #[test]
    fn test_invalid_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'aa', action = 'Quit' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "aa");
        assert!(decoded.bindings.keys[0].with.to_owned().is_empty());
    }

    #[test]
    fn test_mode_key_input() {
        let content = r"
            [bindings]
            keys = [
                { key = 'Home', esc = '\x1bOH', mode = 'appcursor' },
            ]
        ";

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert_eq!(decoded.bindings.keys[0].with, "");
        assert_eq!(decoded.bindings.keys[0].mode, "appcursor");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), "");
        assert!(!decoded.bindings.keys[0].esc.to_owned().is_empty());
    }

    #[test]
    fn test_valid_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Home', esc = '\x1bOH' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert_eq!(decoded.bindings.keys[0].with, "");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), "");
        assert_eq!(decoded.bindings.keys[0].esc, "\\x1bOH");
    }

    #[test]
    fn test_multi_key_actions() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Q', with = 'super', action = 'quit' },
                { key = '+', with = 'super', action = 'increasefontsize' },
                { key = '-', with = 'super', action = 'decreasefontsize' },
                { key = '0', with = 'super', action = 'resetfontsize' },

                { key = '[', with = 'super | shift', action = 'selectnexttab' },
                { key = ']', with = 'super | shift', action = 'selectprevtab' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();

        assert_eq!(decoded.bindings.keys[0].key, "Q");
        assert_eq!(decoded.bindings.keys[0].with, "super");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), "quit");
        assert!(decoded.bindings.keys[0].esc.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[1].key, "+");
        assert_eq!(decoded.bindings.keys[1].with, "super");
        assert_eq!(
            decoded.bindings.keys[1].action.to_owned(),
            "increasefontsize"
        );
        assert!(decoded.bindings.keys[1].esc.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[2].key, "-");
        assert_eq!(decoded.bindings.keys[2].with, "super");
        assert_eq!(
            decoded.bindings.keys[2].action.to_owned(),
            "decreasefontsize"
        );
        assert!(decoded.bindings.keys[2].esc.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[3].key, "0");
        assert_eq!(decoded.bindings.keys[3].with, "super");
        assert_eq!(decoded.bindings.keys[3].action.to_owned(), "resetfontsize");
        assert!(decoded.bindings.keys[3].esc.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[4].key, "[");
        assert_eq!(decoded.bindings.keys[4].with, "super | shift");
        assert_eq!(decoded.bindings.keys[4].action.to_owned(), "selectnexttab");
        assert!(decoded.bindings.keys[4].esc.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[5].key, "]");
        assert_eq!(decoded.bindings.keys[5].with, "super | shift");
        assert_eq!(decoded.bindings.keys[5].action.to_owned(), "selectprevtab");
        assert!(decoded.bindings.keys[5].esc.to_owned().is_empty());
    }

    #[test]
    fn test_escape_sequences() {
        // Test with Unicode escape sequences in double quotes (TOML standard)
        let content = r#"
            [bindings]
            keys = [
                { key = 'l', with = 'control', esc = "\u001b[2J\u001b[H" },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].esc, "\x1b[2J\x1b[H");
        assert_eq!(decoded.bindings.keys[0].esc.as_bytes(), b"\x1b[2J\x1b[H");
    }
}
