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
// Bytes[27, 91, 53, 126] is equivalent to "\x1b[5~"
// { key = "Home", mods: "super | shift", bytes = [27, 91, 53, 126] }

#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct KeyBinding {
    pub key: String,
    #[serde(default = "String::default")]
    pub with: String,
    #[serde(default = "Action::default")]
    pub action: Action,
    #[serde(default = "String::default")]
    pub text: String,
    #[serde(default = "Vec::default")]
    pub bytes: Vec<u8>,
    #[serde(default = "String::default")]
    pub mode: String,
}

pub type KeyBindings = Vec<KeyBinding>;

#[derive(Default, Debug, PartialEq, Clone, Deserialize)]
pub struct Bindings {
    pub keys: KeyBindings,
}

// pub(crate) fn bytes_deserialize<'de, D>(de: D) -> Result<Vec<u8>, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     de.deserialize_byte_buf(ByteBufVisitor)
// }

// struct ByteBufVisitor;

// impl<'de> Visitor<'de> for ByteBufVisitor {
//     type Value = String;

//     fn expecting(&self, out: &mut fmt::Formatter) -> fmt::Result {
//         out.write_str("string")
//     }

//     fn visit_str<E>(self, v: &str) -> Result<String, E>
//     where
//         E: Error,
//     {
//         Ok(v.to_string())
//     }
// }

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
        assert!(decoded.bindings.keys[0].text.to_owned().is_empty());
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
        let content = r#"
            [bindings]
            keys = [
                { key = 'Home', text = '\x1bOH', mode = 'appcursor' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert_eq!(decoded.bindings.keys[0].with, "");
        assert_eq!(decoded.bindings.keys[0].mode, "appcursor");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), Action::None);
        assert!(!decoded.bindings.keys[0].text.to_owned().is_empty());
    }

    #[test]
    fn test_valid_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Home', bytes = [27, 79, 72] },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert_eq!(decoded.bindings.keys[0].with, "");
        assert_eq!(decoded.bindings.keys[0].action.to_owned(), Action::None);
        assert!(decoded.bindings.keys[0].text.to_owned().is_empty());
        let binding = decoded.bindings.keys[0].bytes.to_owned();
        assert_eq!(std::str::from_utf8(&binding).unwrap(), "\x1bOH".to_string());
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
        assert!(decoded.bindings.keys[0].text.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[1].key, "+");
        assert_eq!(decoded.bindings.keys[1].with, "super");
        assert_eq!(
            decoded.bindings.keys[1].action.to_owned(),
            Action::IncreaseFontSize
        );
        assert!(decoded.bindings.keys[1].text.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[2].key, "-");
        assert_eq!(decoded.bindings.keys[2].with, "super");
        assert_eq!(
            decoded.bindings.keys[2].action.to_owned(),
            Action::DecreaseFontSize
        );
        assert!(decoded.bindings.keys[2].text.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[3].key, "0");
        assert_eq!(decoded.bindings.keys[3].with, "super");
        assert_eq!(
            decoded.bindings.keys[3].action.to_owned(),
            Action::ResetFontSize
        );
        assert!(decoded.bindings.keys[3].text.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[4].key, "[");
        assert_eq!(decoded.bindings.keys[4].with, "super | shift");
        assert_eq!(
            decoded.bindings.keys[4].action.to_owned(),
            Action::TabSwitchNext
        );
        assert!(decoded.bindings.keys[4].text.to_owned().is_empty());

        assert_eq!(decoded.bindings.keys[5].key, "]");
        assert_eq!(decoded.bindings.keys[5].with, "super | shift");
        assert_eq!(
            decoded.bindings.keys[5].action.to_owned(),
            Action::TabSwitchPrev
        );
        assert!(decoded.bindings.keys[5].text.to_owned().is_empty());
    }
}
