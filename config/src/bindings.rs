use serde::Deserialize;

#[derive(Debug, Default, Deserialize, PartialEq, Clone, Copy)]
pub enum Action {
    Paste,
    Quit,
    Copy,
    ResetFontSize,
    IncreaseFontSize,
    DecreaseFontSize,
    TabSwitchNext,
    TabSwitchPrev,
    CreateWindow,
    CreateTab,
    #[default]
    None,
}

// { key = "W", mods: "super", action = "Quit" }
// { key = "Home", mods: "super | shift", chars = "\x1b[5~" }

#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    #[serde(default = "String::default")]
    pub with: String,
    #[serde(default = "Action::default")]
    pub action: Action,
    #[serde(default = "String::default")]
    pub input: String,
    #[serde(default = "String::default")]
    pub mode: String,
}

pub type KeyBindings = Vec<KeyBinding>;

#[derive(Default, Debug, PartialEq, Clone, Deserialize)]
pub struct Bindings {
    pub keys: KeyBindings,
}

#[cfg(test)]
mod tests {

    use crate::bindings::{Action, Bindings};
    use serde::Deserialize;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct Root {
        #[serde(default = "Bindings::default")]
        bindings: Bindings,
    }

    #[test]
    fn test_valid_key_action() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Q', with = 'super', action = 'Quit' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Q");
        assert_eq!(decoded.bindings.keys[0].with.to_owned(), "super");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), Action::Quit);
        assert!(decoded.bindings.keys[0].input.to_owned().is_empty());
    }

    #[test]
    fn test_invalid_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'aa', input = '\x1bOH', action = 'Quit' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "aa");
        assert!(decoded.bindings.keys[0].with.to_owned().is_empty());
    }

    #[test]
    fn test_mode_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Home', input = '\x1bOH', mode = 'appcursor' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert_eq!(decoded.bindings.keys[0].with, "");
        assert_eq!(decoded.bindings.keys[0].mode, "appcursor");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), Action::None);
        assert!(!decoded.bindings.keys[0].input.to_owned().is_empty());
        // assert_eq!(decoded.bindings.keys[0].input.to_owned(), "\x1bOH");
    }

    #[test]
    fn test_valid_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Home', input = 'x1bOH' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert_eq!(decoded.bindings.keys[0].with, "");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), Action::None);
        assert!(!decoded.bindings.keys[0].input.to_owned().is_empty());
        // assert_eq!(decoded.bindings.keys[0].input.to_owned(), "\x1bOH");
    }

    #[test]
    fn test_multi_key_actions() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Q', with = 'super', action = 'Quit' },
                { key = '+', with = 'super', action = 'IncreaseFontSize' },
                { key = '-', with = 'super', action = 'DecreaseFontSize' },
                { key = '0', with = 'super', action = 'ResetFontSize' },

                { key = '[', with = 'super | shift', action = 'TabSwitchNext' },
                { key = ']', with = 'super | shift', action = 'TabSwitchPrev' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();

        assert_eq!(decoded.bindings.keys[0].key, "Q");
        assert_eq!(decoded.bindings.keys[0].with, "super");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), Action::Quit);
        assert!(decoded.bindings.keys[0].input.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[1].key, "+");
        assert_eq!(decoded.bindings.keys[1].with, "super");
        assert_eq!(
            decoded.bindings.keys[1].action.to_owned(),
            Action::IncreaseFontSize
        );
        assert!(decoded.bindings.keys[1].input.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[2].key, "-");
        assert_eq!(decoded.bindings.keys[2].with, "super");
        assert_eq!(
            decoded.bindings.keys[2].action.to_owned(),
            Action::DecreaseFontSize
        );
        assert!(decoded.bindings.keys[2].input.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[3].key, "0");
        assert_eq!(decoded.bindings.keys[3].with, "super");
        assert_eq!(
            decoded.bindings.keys[3].action.to_owned(),
            Action::ResetFontSize
        );
        assert!(decoded.bindings.keys[3].input.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[4].key, "[");
        assert_eq!(decoded.bindings.keys[4].with, "super | shift");
        assert_eq!(
            decoded.bindings.keys[4].action.to_owned(),
            Action::TabSwitchNext
        );
        assert!(decoded.bindings.keys[4].input.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[5].key, "]");
        assert_eq!(decoded.bindings.keys[5].with, "super | shift");
        assert_eq!(
            decoded.bindings.keys[5].action.to_owned(),
            Action::TabSwitchPrev
        );
        assert!(decoded.bindings.keys[5].input.to_owned().is_empty());
    }
}
