//! `command-prompt` and `confirm-before`.
//!
//! tmux would draw these on a client's status line, which a control client
//! doesn't have, so the frontend asks instead and this works out what to ask
//! and what to run with the answer.

use super::keys::{next_token, unsupported_command};

/// What kind of answer a prompt takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptKind {
    /// A line of text, ended with Enter.
    Text,
    /// A single key. `enter` says whether Enter counts as that key.
    Confirm { key: char, enter: bool },
}

/// A prompt on its way through its questions, which is usually just one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TmuxPrompt {
    kind: PromptKind,
    labels: Vec<String>,
    inputs: Vec<String>,
    /// The commands to run, with the answers so far filled in.
    template: String,
    answered: usize,
}

impl TmuxPrompt {
    /// Read a `command-prompt` or `confirm-before` command as `list-keys`
    /// shows it. Fails with the name of what can't work on a control client.
    pub fn parse(command: &str) -> Result<TmuxPrompt, String> {
        let (name, mut rest) = next_token(command).ok_or_else(String::new)?;
        let confirm = name == "confirm-before";
        let takes_value = if confirm { "cpt" } else { "IpTt" };

        let mut prompt = None;
        let mut input = None;
        let mut key = 'y';
        let mut enter = false;
        let mut split = true;

        while let Some((token, after)) = next_token(rest) {
            if token == "--" {
                rest = after;
                break;
            }
            let Some(flags) = token.strip_prefix('-').filter(|flags| !flags.is_empty())
            else {
                break;
            };
            rest = after;

            for (index, flag) in flags.char_indices() {
                if takes_value.contains(flag) {
                    let attached = &flags[index + flag.len_utf8()..];
                    let value = if attached.is_empty() {
                        let (value, after) =
                            next_token(rest).ok_or_else(|| name.to_string())?;
                        rest = after;
                        value
                    } else {
                        attached
                    };
                    let value = unquote(value);
                    match flag {
                        'p' => prompt = Some(value),
                        'I' => input = Some(value),
                        'c' => key = value.chars().next().unwrap_or(key),
                        _ => (),
                    }
                    break;
                }
                match flag {
                    // Formats in the template and a reply per key press both
                    // need tmux's own prompt running.
                    'F' | 'i' if !confirm => return Err(name.to_string()),
                    'l' => split = false,
                    'y' => enter = true,
                    _ => (),
                }
            }
        }

        let template = template(rest);
        if let Some(command) = unsupported_command(&template) {
            return Err(command);
        }

        if confirm {
            let label = prompt.unwrap_or_else(|| format!("Confirm '{template}'? (y/n)"));
            return Ok(TmuxPrompt {
                kind: PromptKind::Confirm { key, enter },
                labels: vec![label],
                inputs: vec![String::new()],
                template,
                answered: 0,
            });
        }

        let template = template_or_default(template);
        let list = |text: String| -> Vec<String> {
            if split {
                text.split(',').map(str::to_string).collect()
            } else {
                vec![text]
            }
        };
        let labels = match prompt {
            Some(prompt) => list(prompt),
            None if template == DEFAULT_TEMPLATE => vec![String::from(":")],
            None => {
                let first = template.split_whitespace().next().unwrap_or_default();
                vec![format!("({first})")]
            }
        };
        let mut inputs = input.map(list).unwrap_or_default();
        inputs.resize(labels.len(), String::new());

        Ok(TmuxPrompt {
            kind: PromptKind::Text,
            labels,
            inputs,
            template,
            answered: 0,
        })
    }

    pub fn kind(&self) -> PromptKind {
        self.kind
    }

    /// What the current question says.
    pub fn label(&self) -> &str {
        self.labels.get(self.answered).map_or("", String::as_str)
    }

    /// The text the current question starts with.
    pub fn input(&self) -> &str {
        self.inputs.get(self.answered).map_or("", String::as_str)
    }

    /// The commands of a confirmation.
    pub fn commands(&self) -> &str {
        &self.template
    }

    /// The labels and inputs for tmux to expand, or nothing if none of them
    /// have formats in them.
    pub fn formats(&self) -> Vec<String> {
        let texts = self.labels.iter().chain(&self.inputs);
        if texts.clone().any(|text| text.contains('#')) {
            texts.cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Take the expansions of [`Self::formats`].
    pub fn set_formats(&mut self, expanded: &[String]) {
        if expanded.len() != self.labels.len() + self.inputs.len() {
            return;
        }
        let (labels, inputs) = expanded.split_at(self.labels.len());
        self.labels = labels.to_vec();
        self.inputs = inputs.to_vec();
    }

    /// Answer the current question. Returns the commands to run once there
    /// are no questions left.
    pub fn answer(&mut self, text: &str) -> Option<String> {
        self.answered += 1;
        self.template = replace(&self.template, text, self.answered);
        (self.answered >= self.labels.len()).then(|| self.template.clone())
    }
}

/// What a `command-prompt` with no template runs: the answer, as commands.
const DEFAULT_TEMPLATE: &str = "%1";

fn template_or_default(template: String) -> String {
    if template.is_empty() {
        String::from(DEFAULT_TEMPLATE)
    } else {
        template
    }
}

/// The template after a command's flags: a quoted string, or `{ commands }`
/// from tmux 3.3 on.
fn template(rest: &str) -> String {
    let rest = rest.trim();
    if let Some(inner) = rest.strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        return inner.trim().to_string();
    }
    match next_token(rest) {
        Some((token, after)) if after.trim().is_empty() => unquote(token),
        _ => rest.to_string(),
    }
}

/// Undo a token's quoting, the way tmux reads it.
fn unquote(token: &str) -> String {
    let mut text = String::with_capacity(token.len());
    let mut quote = None;
    let mut chars = token.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(open), c) if c == open => quote = None,
            (None, '\'' | '"') => quote = Some(c),
            (None | Some('"'), '\\') => text.extend(chars.next()),
            (_, c) => text.push(c),
        }
    }
    text
}

/// Quote `text` so that tmux reads it back as one argument, unchanged.
pub(super) fn quote(text: &str) -> String {
    let text: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Fill in answer number `index`, as tmux does: `%1` to `%9` go by number and
/// the first `%%` takes the answer too. A further `%` escapes the answer.
fn replace(template: &str, answer: &str, index: usize) -> String {
    let number = u8::try_from(index).ok().filter(|n| (1..=9).contains(n));
    let mut text = String::with_capacity(template.len() + answer.len());
    let mut rest = template;
    let mut replaced = false;

    while let Some(position) = rest.find('%') {
        text.push_str(&rest[..position]);
        rest = &rest[position..];

        let next = rest.as_bytes().get(1).copied();
        let numbered = next.is_some_and(|next| Some(next.wrapping_sub(b'0')) == number);
        let first = next == Some(b'%') && !replaced;
        if !numbered && !first {
            text.push('%');
            rest = &rest[1..];
            continue;
        }
        replaced |= !numbered;
        rest = &rest[2..];

        let escaped = rest.starts_with('%');
        if escaped {
            rest = &rest[1..];
        }
        for c in answer.chars() {
            if escaped && "\"\\$;~".contains(c) {
                text.push('\\');
            }
            text.push(c);
        }
    }
    text.push_str(rest);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(command: &str) -> TmuxPrompt {
        TmuxPrompt::parse(command).unwrap()
    }

    #[test]
    fn a_bare_prompt_runs_what_is_typed() {
        let mut prompt = parse("command-prompt");
        assert_eq!(prompt.kind(), PromptKind::Text);
        assert_eq!(prompt.label(), ":");
        assert_eq!(prompt.input(), "");
        assert!(prompt.formats().is_empty());
        assert_eq!(
            prompt.answer("split-window -h").as_deref(),
            Some("split-window -h")
        );
    }

    #[test]
    fn answers_go_into_the_template() {
        let mut prompt = parse(r##"command-prompt -I "#W" "rename-window -- '%%'""##);
        assert_eq!(prompt.label(), "(rename-window)");
        assert_eq!(prompt.input(), "#W");
        assert_eq!(
            prompt.answer("logs").as_deref(),
            Some("rename-window -- 'logs'")
        );
    }

    #[test]
    fn templates_can_be_braced() {
        let mut prompt = parse(r##"command-prompt -I "#S" { rename-session "%%" }"##);
        assert_eq!(
            prompt.answer("work").as_deref(),
            Some(r#"rename-session "work""#)
        );
    }

    #[test]
    fn flags_take_values_attached_or_clustered() {
        let prompt = parse(r#"command-prompt -1p "index" "select-window -t ':%%'""#);
        assert_eq!(prompt.label(), "index");
        let prompt = parse(r#"command-prompt -T window-target -pindex "a %%""#);
        assert_eq!(prompt.label(), "index");
        assert_eq!(prompt.commands(), "a %%");
    }

    #[test]
    fn several_questions_are_asked_in_turn() {
        let mut prompt =
            parse(r#"command-prompt -p "from,to" -I "a,b" "move-pane -s %1 -t %2""#);
        assert_eq!((prompt.label(), prompt.input()), ("from", "a"));
        assert_eq!(prompt.answer("1"), None);
        assert_eq!((prompt.label(), prompt.input()), ("to", "b"));
        assert_eq!(prompt.answer("2").as_deref(), Some("move-pane -s 1 -t 2"));
    }

    #[test]
    fn inputs_missing_from_the_list_are_empty() {
        let mut prompt = parse(r#"command-prompt -p "a,b" -I "x" "run %1 %2""#);
        prompt.answer("1");
        assert_eq!(prompt.input(), "");
    }

    #[test]
    fn literal_prompts_are_not_split() {
        let prompt = parse(r#"command-prompt -l -p "a,b" "run %%""#);
        assert_eq!(prompt.label(), "a,b");
    }

    #[test]
    fn formats_are_expanded_as_a_set() {
        let mut prompt =
            parse(r##"command-prompt -p "(#S)" -I "#W" "rename-window '%%'""##);
        assert_eq!(prompt.formats(), ["(#S)", "#W"]);
        prompt.set_formats(&["(main)".into(), "logs".into()]);
        assert_eq!((prompt.label(), prompt.input()), ("(main)", "logs"));

        prompt.set_formats(&["wrong".into()]);
        assert_eq!(prompt.label(), "(main)");
    }

    #[test]
    fn confirmations_keep_their_command() {
        let prompt = parse(r##"confirm-before -p "kill-pane #P? (y/n)" kill-pane"##);
        assert_eq!(
            prompt.kind(),
            PromptKind::Confirm {
                key: 'y',
                enter: false
            }
        );
        assert_eq!(prompt.label(), "kill-pane #P? (y/n)");
        assert_eq!(prompt.commands(), "kill-pane");
        assert_eq!(prompt.formats().len(), 2);
    }

    #[test]
    fn confirmations_have_a_default_question_and_options() {
        let prompt = parse(r#"confirm-before -y -c k "kill-window""#);
        assert_eq!(
            prompt.kind(),
            PromptKind::Confirm {
                key: 'k',
                enter: true
            }
        );
        assert_eq!(prompt.label(), "Confirm 'kill-window'? (y/n)");
    }

    #[test]
    fn prompts_that_need_tmux_are_refused() {
        assert_eq!(
            TmuxPrompt::parse(r#"command-prompt -i "send-keys %%""#),
            Err(String::from("command-prompt"))
        );
        assert_eq!(
            TmuxPrompt::parse(r#"command-prompt -F "rename-window #W""#),
            Err(String::from("command-prompt"))
        );
        assert_eq!(
            TmuxPrompt::parse(r#"command-prompt "find-window -Z -- '%%'""#),
            Err(String::from("find-window"))
        );
    }

    #[test]
    fn replacing_follows_tmux() {
        assert_eq!(replace("a %% b %%", "x", 1), "a x b %%");
        assert_eq!(replace("%1 %1 %2", "x", 1), "x x %2");
        assert_eq!(replace("%2 %%", "y", 2), "y y");
        assert_eq!(replace("100% done", "x", 1), "100% done");
        assert_eq!(replace("end %", "x", 1), "end %");
        assert_eq!(replace("say %%%", r#"a"b;c"#, 1), r#"say a\"b\;c"#);
        assert_eq!(replace("é %% ü", "ñ", 1), "é ñ ü");
    }

    #[test]
    fn quoting_survives_quotes_and_control_bytes() {
        assert_eq!(quote("#W"), "'#W'");
        assert_eq!(quote("it's"), r"'it'\''s'");
        assert_eq!(quote("a\nb"), "'a b'");
    }
}
