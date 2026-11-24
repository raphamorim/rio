use serde::{Deserialize, Serialize};
use std::path::Path;

use super::defaults::{default_title_content, default_title_placeholder};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub struct Title {
    #[serde(default = "default_title_placeholder")]
    pub placeholder: Option<String>,
    #[serde(default = "default_title_content")]
    pub content: String,
}

#[allow(clippy::derivable_impls)]
impl Default for Title {
    fn default() -> Title {
        Title {
            placeholder: default_title_placeholder(),
            content: default_title_content(),
        }
    }
}

/// What a title template's variables stand for.
pub struct TitleVars<'a> {
    /// The terminal's title, from OSC 0/2.
    pub title: &'a str,
    /// The command the pane spawned.
    pub program: &'a str,
    /// The working directory, from OSC 7.
    pub directory: Option<&'a std::path::Path>,
    pub columns: usize,
    pub lines: usize,
}

// Template variables:
//
// - `TITLE`: terminal title via OSC sequences for setting terminal title
// - `PROGRAM`: the command the pane spawned (e.g `fish`, `zsh`, `bash`)
// - `ABSOLUTE_PATH`: working directory via OSC 7
// - `RELATIVE_PATH`: OSC 7 directory, home-relative (e.g `~/a/rio` or `…/a/b/c`)
// - `COLUMNS`: current columns
// - `LINES`: current lines

/// Shorten an absolute path for display:
/// - Replace home directory prefix with `~`
/// - If 4+ components deep, show `…/last/three/components`
pub fn shorten_path(absolute: &str) -> String {
    let path = Path::new(absolute);

    // Replace home prefix with ~
    #[cfg(unix)]
    let display_path = {
        if let Some(home) = dirs::home_dir() {
            if let Ok(stripped) = path.strip_prefix(&home) {
                let s = stripped.to_string_lossy();
                if s.is_empty() {
                    "~".to_string()
                } else {
                    format!("~/{s}")
                }
            } else {
                absolute.to_string()
            }
        } else {
            absolute.to_string()
        }
    };

    #[cfg(not(unix))]
    let display_path = absolute.to_string();

    // If 4+ components, show …/last3
    let components: Vec<&str> =
        display_path.split('/').filter(|s| !s.is_empty()).collect();
    if components.len() >= 4 {
        format!("…/{}", components[components.len() - 3..].join("/"))
    } else {
        display_path
    }
}

/// Whether the title has `{{ }}` variables to render; anything else is
/// shown as-is.
#[inline]
pub fn is_template(title: &str) -> bool {
    title.contains("{{")
}

/// Render a title template, `{{ VARIABLE || FALLBACK }}` and all.
pub fn render(template: &str, vars: &TitleVars<'_>) -> String {
    if !is_template(template) {
        return template.to_string();
    }
    let mut new_template = template.to_owned();
    // Compiled once: titles are now rendered per OSC title change, not on
    // a 2s poll, so a per-call `Regex::new` would sit in the hot path.
    static RE: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\{\{(.*?)\}\}").unwrap());
    for (to_replace_str, [variable]) in RE.captures_iter(template).map(|c| c.extract()) {
        let variables = if to_replace_str.contains("||") {
            variable.split("||").collect()
        } else {
            vec![variable]
        };

        let mut matched = false;
        for (i, scoped_variable) in variables.iter().enumerate() {
            if matched {
                break;
            }

            let var = scoped_variable.to_owned().trim().to_lowercase();
            match var.as_str() {
                "columns" => {
                    new_template =
                        new_template.replace(to_replace_str, &vars.columns.to_string());
                    matched = true;
                }
                "lines" => {
                    new_template =
                        new_template.replace(to_replace_str, &vars.lines.to_string());
                    matched = true;
                }
                "title" => {
                    // In case it has a fallback and title is empty
                    // or
                    // In case is the last then we need to erase variables either way
                    let is_only_one = variables.len() == 1;
                    let is_last = i == variables.len() - 1;
                    if is_only_one || is_last {
                        new_template = new_template.replace(to_replace_str, vars.title);
                        continue;
                    }

                    if !vars.title.is_empty() {
                        new_template = new_template.replace(to_replace_str, vars.title);
                        matched = true;
                    }
                }
                "program" => {
                    new_template = new_template.replace(to_replace_str, vars.program);
                    matched = true;
                }
                "absolute_path" => {
                    let path = vars
                        .directory
                        .map(|d| d.to_string_lossy().into_owned())
                        .unwrap_or_default();

                    let is_only_one = variables.len() == 1;
                    let is_last = i == variables.len() - 1;
                    if is_only_one || is_last {
                        new_template = new_template.replace(to_replace_str, &path);
                        continue;
                    }

                    if !path.is_empty() {
                        new_template = new_template.replace(to_replace_str, &path);
                        matched = true;
                    }
                }
                "relative_path" => {
                    let path = vars
                        .directory
                        .map(|d| shorten_path(&d.to_string_lossy()))
                        .unwrap_or_default();

                    let is_only_one = variables.len() == 1;
                    let is_last = i == variables.len() - 1;
                    if is_only_one || is_last {
                        new_template = new_template.replace(to_replace_str, &path);
                        continue;
                    }

                    if !path.is_empty() {
                        new_template = new_template.replace(to_replace_str, &path);
                        matched = true;
                    }
                }
                _ => {}
            }
        }
    }

    new_template
}
