use crate::context::Context;
use rustc_hash::FxHashMap;
use std::time::Instant;

pub struct ContextTitle {
    pub content: String,
    pub current_path: String,
}

pub struct ContextManagerTitles {
    pub last_title_update: Option<Instant>,
    pub titles: FxHashMap<usize, ContextTitle>,
    pub key: String,
}

impl ContextManagerTitles {
    pub fn new(
        idx: usize,
        content: String,
        current_path: String,
    ) -> ContextManagerTitles {
        let key = format!("{}{};", idx, content);
        let mut map = FxHashMap::default();
        map.insert(
            idx,
            ContextTitle {
                content,
                current_path,
            },
        );
        ContextManagerTitles {
            key,
            titles: map,
            last_title_update: None,
        }
    }

    #[inline]
    pub fn set_key_val(&mut self, idx: usize, content: String, current_path: String) {
        self.titles.insert(
            idx,
            ContextTitle {
                content,
                current_path,
            },
        );
    }

    #[inline]
    pub fn set_key(&mut self, key: String) {
        self.key = key;
    }
}

// Possible options:

// - `TITLE`: terminal title via OSC sequences for setting terminal title
// - `PROGRAM`: (e.g `fish`, `zsh`, `bash`, `vim`, etc...)
// - `PATH_ABSOLUTE`: (e.g `/Users/rapha/Documents/a/rio`)
// - `PATH_RELATIVE`: (e.g `.../Documents/a/rio`, `~/Documents/a`)
// - `COLUMNS`: current columns
// - `LINES`: current lines

#[inline]
pub fn update_title<T: rio_backend::event::EventListener>(
    template: &str,
    context: &Context<T>,
) -> String {
    if template.is_empty() {
        return template.to_string();
    }

    let mut new_template = template.to_owned();

    let re = regex::Regex::new(r"\{\{(.*?)\}\}").unwrap();
    for (to_replace_str, [variable]) in re.captures_iter(template).map(|c| c.extract()) {
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
                    let terminal_title = {
                        let terminal = context.terminal.lock();
                        terminal.title.to_string()
                    };

                    println!("{:?}", terminal_title);

                    // In case it has a fallback and title is empty
                    // or
                    // In case is the last then we need to erase variables either way
                    if variables.len() <= 1 || i == variables.len() - 1 {
                        new_template =
                            new_template.replace(to_replace_str, &terminal_title);
                    } else if !terminal_title.is_empty() {
                        matched = true;
                    }
                }
                "program" => {
                    #[cfg(unix)]
                    {
                        let program = teletypewriter::foreground_process_name(
                            *context.main_fd,
                            context.shell_pid,
                        );

                        new_template = new_template.replace(to_replace_str, &program);
                    }
                }
                "path_absolute" => {
                    #[cfg(unix)]
                    {
                        let path = teletypewriter::foreground_process_path(
                            *context.main_fd,
                            context.shell_pid,
                        )
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                        new_template = new_template.replace(to_replace_str, &path);
                    }
                }
                // TODO:
                // "path_relative" => {
                //     #[cfg(unix)]
                //     {
                //         let path = teletypewriter::foreground_process_path(
                //             *context.main_fd,
                //             context.shell_pid,
                //         )
                //         .map(|p| p.canonicalize().unwrap_or_default().to_string_lossy().to_string())
                //         .unwrap_or_default();
                //         new_template = new_template.replace(to_replace_str, &path);
                //     }
                // },
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
    use crate::context::Delta;
    use rio_backend::event::VoidListener;
    use rio_backend::sugarloaf::layout::SugarDimensions;
    use rio_window::window::WindowId;

    #[test]
    fn test_update_title() {
        let context_dimension = ContextDimension::build(
            1200.0,
            800.0,
            SugarDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 66);
        assert_eq!(context_dimension.lines, 88);

        let rich_text_id = 0;
        let route_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            route_id,
            rich_text_id,
            context_dimension,
        );
        assert_eq!(update_title("", &context), String::from(""));
        assert_eq!(update_title("{{columns}}", &context), String::from("66"));
        assert_eq!(update_title("{{COLUMNS}}", &context), String::from("66"));
        assert_eq!(update_title("{{ COLUMNS }}", &context), String::from("66"));
        assert_eq!(update_title("{{ columns }}", &context), String::from("66"));
        assert_eq!(
            update_title("hello {{ COLUMNS }} AbC", &context),
            String::from("hello 66 AbC")
        );
        assert_eq!(
            update_title("hello {{ Lines }} AbC", &context),
            String::from("hello 88 AbC")
        );
        assert_eq!(
            update_title("{{ columns }}x{{lines}", &context),
            String::from("66x88")
        );

        assert_eq!(update_title("{{ title }}", &context), String::from(""));

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
            SugarDimensions {
                scale: 2.,
                width: 18.,
                height: 9.,
            },
            1.0,
            Delta::<f32>::default(),
        );

        assert_eq!(context_dimension.columns, 66);
        assert_eq!(context_dimension.lines, 88);

        let rich_text_id = 0;
        let route_id = 0;
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
            route_id,
            rich_text_id,
            context_dimension,
        );
        assert_eq!(update_title("", &context), String::from(""));
        // Title always starts empty
        assert_eq!(update_title("{{title}}", &context), String::from(""));

        assert_eq!(
            update_title("{{ title || columns }}", &context),
            String::from("66")
        );

        assert_eq!(
            update_title("{{ columns || title }}", &context),
            String::from("66")
        );

        assert_eq!(
            update_title("{{ title || title }}", &context),
            String::from("")
        );
    }
}
