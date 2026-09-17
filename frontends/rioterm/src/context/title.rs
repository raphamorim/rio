use crate::context::Context;
use std::path::Path;

#[derive(PartialEq)]
pub struct ContextTitle {
    pub content: String,
}

impl Default for ContextTitle {
    fn default() -> Self {
        Self {
            content: String::from("~"),
        }
    }
}

// Possible options:

// - `TITLE`: terminal title via OSC sequences for setting terminal title
// - `PROGRAM`: the command the pane spawned (e.g `fish`, `zsh`, `bash`)
// - `ABSOLUTE_PATH`: working directory via OSC 7 (e.g `/Users/rapha/Documents/a/rio`)
// - `RELATIVE_PATH`: OSC 7 directory, home-relative (e.g `~/Documents/a/rio` or `…/a/psone/starpsx`)
// - `COLUMNS`: current columns
// - `LINES`: current lines

/// Shorten an absolute path for display:
/// - Replace home directory prefix with `~`
/// - If 4+ components deep, show `…/last/three/components`
fn shorten_path(absolute: &str) -> String {
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

#[inline]
/// Render the title template. Every variable is event-known, so this
/// NEVER inspects the foreground process: `{{ title }}` is OSC 0/2,
/// the path variables are OSC 7 (empty for shells without
/// integration), `{{ program }}` is the name of the command the pane
/// spawned, and columns/lines are the pane's own dimensions.
/// `prefetched_title` reuses the OSC title string the caller already
/// holds (a `Title` event carries it), so a `{{ title }}` render off
/// an event never locks the terminal; otherwise one lock fetches
/// title and cwd together.
pub fn update_title<T: rio_backend::event::EventListener>(
    template: &str,
    context: &Context<T>,
    prefetched_title: Option<&str>,
) -> String {
    if template.is_empty() {
        return template.to_string();
    }

    let mut new_template = template.to_owned();
    let lowered = template.to_lowercase();
    let needs_title = lowered.contains("title");
    let needs_path = lowered.contains("path");
    let (terminal_title, current_directory) =
        if (needs_title && prefetched_title.is_none()) || needs_path {
            let terminal = context.terminal.lock();
            (
                match prefetched_title {
                    Some(title) => title.to_string(),
                    None => terminal.title.to_string(),
                },
                terminal.current_directory.clone(),
            )
        } else {
            (prefetched_title.unwrap_or_default().to_string(), None)
        };
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
                    new_template = new_template
                        .replace(to_replace_str, &context.dimension.columns.to_string());
                    matched = true;
                }
                "lines" => {
                    new_template = new_template
                        .replace(to_replace_str, &context.dimension.lines.to_string());
                    matched = true;
                }
                "title" => {
                    // In case it has a fallback and title is empty
                    // or
                    // In case is the last then we need to erase variables either way
                    let is_only_one = variables.len() == 1;
                    let is_last = i == variables.len() - 1;
                    if is_only_one || is_last {
                        new_template =
                            new_template.replace(to_replace_str, &terminal_title);
                        continue;
                    }

                    if !terminal_title.is_empty() {
                        new_template =
                            new_template.replace(to_replace_str, &terminal_title);
                        matched = true;
                    }
                }
                "program" => {
                    new_template =
                        new_template.replace(to_replace_str, &context.spawned_program);
                    matched = true;
                }
                "absolute_path" => {
                    let path = current_directory
                        .as_ref()
                        .and_then(|d| d.clone().into_os_string().into_string().ok())
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
                    let path = current_directory
                        .as_ref()
                        .and_then(|d| d.clone().into_os_string().into_string().ok())
                        .map(|d| shorten_path(&d))
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

#[cfg(test)]
pub mod test {
    use super::*;
    use crate::context::create_mock_context;
    use crate::context::ContextDimension;
    use rio_backend::config::layout::Margin;
    use rio_backend::event::VoidListener;
    use rio_backend::event::WindowId;
    use rio_backend::sugarloaf::layout::TextDimensions;

    #[test]
    fn test_update_title() {
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            TextDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            rio_backend::sugarloaf::layout::CellMetrics {
                cell_width: 18,
                cell_height: 9,
                cell_baseline: 0,
                face_width: 18.0,
                face_height: 9.0,
                face_y: 0.0,
            },
            1.0,
            14.0,
            Margin::default(),
        );

        assert_eq!(context_dimension.columns, 64);
        assert_eq!(context_dimension.lines, 84);

        let rich_text_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            rich_text_id,
            context_dimension,
        );
        assert_eq!(update_title("", &context, None), String::from(""));
        assert_eq!(
            update_title("{{columns}}", &context, None),
            String::from("64")
        );
        assert_eq!(
            update_title("{{COLUMNS}}", &context, None),
            String::from("64")
        );
        assert_eq!(
            update_title("{{ COLUMNS }}", &context, None),
            String::from("64")
        );
        assert_eq!(
            update_title("{{ columns }}", &context, None),
            String::from("64")
        );
        assert_eq!(
            update_title("hello {{ COLUMNS }} AbC", &context, None),
            String::from("hello 64 AbC")
        );
        assert_eq!(
            update_title("hello {{ Lines }} AbC", &context, None),
            String::from("hello 84 AbC")
        );
        assert_eq!(
            update_title("{{ columns }}x{{lines}}", &context, None),
            String::from("64x84")
        );

        assert_eq!(
            update_title("{{ title }}", &context, None),
            String::from("")
        );

        // #[cfg(unix)]
        // assert_eq!(
        //     update_title("{{path_absolute}}"), &context)
        //     String::from("")
        // );
    }

    #[test]
    fn test_update_title_with_logical_or() {
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            TextDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            rio_backend::sugarloaf::layout::CellMetrics {
                cell_width: 18,
                cell_height: 9,
                cell_baseline: 0,
                face_width: 18.0,
                face_height: 9.0,
                face_y: 0.0,
            },
            1.0,
            14.0,
            Margin::default(),
        );

        assert_eq!(context_dimension.columns, 64);
        assert_eq!(context_dimension.lines, 84);

        let rich_text_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            rich_text_id,
            context_dimension,
        );
        assert_eq!(update_title("", &context, None), String::from(""));
        // Title always starts empty
        assert_eq!(update_title("{{title}}", &context, None), String::from(""));

        assert_eq!(
            update_title("{{ title || columns }}", &context, None),
            String::from("64")
        );

        assert_eq!(
            update_title("{{ title || title }}", &context, None),
            String::from("")
        );

        // let's modify title to actually be something
        {
            let mut term = context.terminal.lock();
            term.title = "Something".to_string();
        };

        assert_eq!(
            update_title("{{ title || columns }}", &context, None),
            String::from("Something")
        );

        assert_eq!(
            update_title("{{ columns || title }}", &context, None),
            String::from("64")
        );

        // Use a path that can't plausibly be $HOME on any realistic system.
        // Sandboxed builds (e.g. Void's xbps-src) often set HOME=/tmp, so a
        // literal "/tmp" here would get collapsed to "~" and break the test.
        {
            let path = std::path::PathBuf::from("/rio-sandbox-test-dir");
            let mut term = context.terminal.lock();
            term.current_directory = Some(path);
        };

        assert_eq!(
            update_title("{{ absolute_path || title }}", &context, None),
            String::from("/rio-sandbox-test-dir"),
        );

        assert_eq!(
            update_title("{{ relative_path || title }}", &context, None),
            String::from("/rio-sandbox-test-dir"),
        );
    }

    #[test]
    fn test_update_title_program_is_spawned_command() {
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            TextDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            rio_backend::sugarloaf::layout::CellMetrics {
                cell_width: 18,
                cell_height: 9,
                cell_baseline: 0,
                face_width: 18.0,
                face_height: 9.0,
                face_y: 0.0,
            },
            1.0,
            14.0,
            Margin::default(),
        );

        let mut context =
            create_mock_context(VoidListener {}, WindowId::from(0), 0, context_dimension);
        context.spawned_program = "fish".to_string();

        assert_eq!(update_title("{{ program }}", &context, None), "fish");
        assert_eq!(
            update_title("{{ title || program }}", &context, None),
            "fish"
        );

        // Path variables come from OSC 7 alone: without integration
        // they render empty instead of inspecting the process.
        assert_eq!(update_title("{{ relative_path }}", &context, None), "");
        assert_eq!(update_title("{{ absolute_path }}", &context, None), "");

        // A prefetched OSC title renders without touching the terminal.
        assert_eq!(update_title("{{ title }}", &context, Some("t")), "t");
    }

    #[test]
    fn test_shorten_path() {
        // Use a path prefix that can't plausibly be $HOME to keep the test
        // deterministic in build sandboxes that set HOME=/tmp or similar.
        assert_eq!(
            shorten_path("/rio-sandbox-test-dir"),
            "/rio-sandbox-test-dir",
        );
        assert_eq!(
            shorten_path("/rio-sandbox-test-dir/sub"),
            "/rio-sandbox-test-dir/sub",
        );

        // Deep paths get truncated to last 3 components
        assert_eq!(shorten_path("/a/b/c/d/e"), "…/c/d/e");
        assert_eq!(shorten_path("/a/b/c/d"), "…/b/c/d");

        // 3 components stays as-is
        assert_eq!(shorten_path("/a/b/c"), "/a/b/c");
    }
}
