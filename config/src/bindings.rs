use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum With {
    Super,
    Shift,
    Control,
    Option,
    Alt,
}

// key_bindings:
// - { key: V,        mods: Control|Shift, action: Paste            }
// - { key: V,        mods: Command, action: Paste                        }
// - { key: C,        mods: Command, action: Copy                         }
// - { key: Q,        mods: Command, action: Quit                         }
// - { key: W,        mods: Command, action: Quit                         }
// - { key: Home,                    chars: "\x1bOH",   mode: AppCursor   }
// - { key: Home,                    chars: "\x1b[H",   mode: ~AppCursor  }
// - { key: End,                     chars: "\x1bOF",   mode: AppCursor   }
// - { key: End,                     chars: "\x1b[F",   mode: ~AppCursor  }
// - { key: Key0,     mods: Command, action: ResetFontSize                }
// - { key: Equals,   mods: Command, action: IncreaseFontSize             }
// - { key: Minus,    mods: Command, action: DecreaseFontSize             }
// - { key: PageUp,   mods: Shift,   chars: "\x1b[5;2~"                   }
// - { key: PageUp,   mods: Control, chars: "\x1b[5;5~"                   }
// - { key: PageUp,                  chars: "\x1b[5~"                     }
// - { key: PageDown, mods: Shift,   chars: "\x1b[6;2~"                   }

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Action {
    Paste,
    Quit,
    Copy,
    ResetFontSize,
    IncreaseFontSize,
    DecreaseFontSize,
    TabSwitchNext,
    TabSwitchPrev,
}

#[derive(Debug, Deserialize, PartialEq, Clone, Copy)]
pub enum Mode {
    AppCursor,
    AppKeypad,
    AltScreen,
    VI,
}

// { key = "W", mods: ["Command"], action = "Quit" }
// { key = "Home", mods: ["Command", "Shift"], chars = "\x1b[5~" }

#[derive(Debug, PartialEq, Clone, Deserialize)]
pub struct KeyBinding {
    key: String,
    with: Option<Vec<With>>,
    action: Option<Action>,
    input: Option<String>,
    mode: Option<Mode>,
}

pub type KeyBindings = Vec<KeyBinding>;

#[derive(Default, Debug, PartialEq, Clone, Deserialize)]
pub struct Bindings {
    keys: KeyBindings,
}

#[cfg(test)]
mod tests {

    use crate::bindings::{Action, Bindings, With};
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
                { key = 'Q', with = ['Super'], action = 'Quit' }
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Q");
        assert_eq!(
            decoded.bindings.keys[0].with.to_owned().unwrap(),
            vec![With::Super]
        );
        assert_eq!(
            decoded.bindings.keys[0].action.to_owned().unwrap(),
            Action::Quit
        );
        assert!(decoded.bindings.keys[0].input.to_owned().is_none());
    }

    // #[test]
    // fn test_invalid_key_input() {
    //     let content = r#"
    //         [bindings]
    //         keys = [
    //             { key = 'aa', input = '\x1bOH', action = 'Quita' },
    //         ]
    //     "#;

    //     let decoded = toml::from_str::<Root>(content).unwrap();
    //     assert_eq!(decoded.bindings.keys[0].key, "Home");
    //     assert!(decoded.bindings.keys[0].with.to_owned().is_none());
    //     assert!(decoded.bindings.keys[0].action.to_owned().is_none());
    //     assert!(decoded.bindings.keys[0].input.to_owned().is_some());
    //     // assert_eq!(decoded.bindings.keys[0].input.to_owned().unwrap(), "\x1bOH");
    // }

    #[test]
    fn test_valid_key_input() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Home', input = '\x1bOH' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();
        assert_eq!(decoded.bindings.keys[0].key, "Home");
        assert!(decoded.bindings.keys[0].with.to_owned().is_none());
        assert!(decoded.bindings.keys[0].action.to_owned().is_none());
        assert!(decoded.bindings.keys[0].input.to_owned().is_some());
        // assert_eq!(decoded.bindings.keys[0].input.to_owned().unwrap(), "\x1bOH");
    }

    #[test]
    fn test_multi_key_actions() {
        let content = r#"
            [bindings]
            keys = [
                { key = 'Q', with = ['Super'], action = 'Quit' },
                { key = '+', with = ['Super'], action = 'IncreaseFontSize' },
                { key = '-', with = ['Super'], action = 'DecreaseFontSize' },
                { key = '0', with = ['Super'], action = 'ResetFontSize' },

                { key = 'LBracket', with = ['Super', 'Shift'], action = 'TabSwitchNext' },
                { key = 'RBracket', with = ['Super', 'Shift'], action = 'TabSwitchPrev' },
            ]
        "#;

        let decoded = toml::from_str::<Root>(content).unwrap();

        assert_eq!(decoded.bindings.keys[0].key, "Q");
        assert_eq!(
            decoded.bindings.keys[0].with.to_owned().unwrap(),
            vec![With::Super]
        );
        assert_eq!(
            decoded.bindings.keys[0].action.to_owned().unwrap(),
            Action::Quit
        );
        assert!(decoded.bindings.keys[0].input.to_owned().is_none());

        assert_eq!(decoded.bindings.keys[1].key, "+");
        assert_eq!(
            decoded.bindings.keys[1].with.to_owned().unwrap(),
            vec![With::Super]
        );
        assert_eq!(
            decoded.bindings.keys[1].action.to_owned().unwrap(),
            Action::IncreaseFontSize
        );
        assert!(decoded.bindings.keys[1].input.to_owned().is_none());

        assert_eq!(decoded.bindings.keys[2].key, "-");
        assert_eq!(
            decoded.bindings.keys[2].with.to_owned().unwrap(),
            vec![With::Super]
        );
        assert_eq!(
            decoded.bindings.keys[2].action.to_owned().unwrap(),
            Action::DecreaseFontSize
        );
        assert!(decoded.bindings.keys[2].input.to_owned().is_none());

        assert_eq!(decoded.bindings.keys[3].key, "0");
        assert_eq!(
            decoded.bindings.keys[3].with.to_owned().unwrap(),
            vec![With::Super]
        );
        assert_eq!(
            decoded.bindings.keys[3].action.to_owned().unwrap(),
            Action::ResetFontSize
        );
        assert!(decoded.bindings.keys[3].input.to_owned().is_none());

        assert_eq!(decoded.bindings.keys[4].key, "LBracket");
        assert_eq!(
            decoded.bindings.keys[4].with.to_owned().unwrap(),
            vec![With::Super, With::Shift]
        );
        assert_eq!(
            decoded.bindings.keys[4].action.to_owned().unwrap(),
            Action::TabSwitchNext
        );
        assert!(decoded.bindings.keys[4].input.to_owned().is_none());

        assert_eq!(decoded.bindings.keys[5].key, "RBracket");
        assert_eq!(
            decoded.bindings.keys[5].with.to_owned().unwrap(),
            vec![With::Super, With::Shift]
        );
        assert_eq!(
            decoded.bindings.keys[5].action.to_owned().unwrap(),
            Action::TabSwitchPrev
        );
        assert!(decoded.bindings.keys[5].input.to_owned().is_none());
    }
}
