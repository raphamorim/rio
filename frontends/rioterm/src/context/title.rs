use crate::context::Context;
use rustc_hash::FxHashMap;
use std::time::Instant;

pub struct ContextTitleExtra {
    pub program: String,
    pub path: String,
}

pub struct ContextTitle {
    pub content: String,
    pub extra: Option<ContextTitleExtra>,
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
        extra: Option<ContextTitleExtra>,
    ) -> ContextManagerTitles {
        let key = format!("{idx}{content};");
        let mut map = FxHashMap::default();
        map.insert(idx, ContextTitle { content, extra });
        ContextManagerTitles {
            key,
            titles: map,
            last_title_update: None,
        }
    }

    #[inline]
    pub fn set_key_val(
        &mut self,
        idx: usize,
        content: String,
        extra: Option<ContextTitleExtra>,
    ) {
        self.titles.insert(idx, ContextTitle { content, extra });
    }

    #[inline]
    pub fn set_key(&mut self, key: String) {
        self.key = key;
    }
}

pub fn create_title_extra_from_context<T: rio_backend::event::EventListener>(
    context: &Context<T>,
) -> Option<ContextTitleExtra> {
    #[cfg(unix)]
    let path =
        teletypewriter::foreground_process_path(*context.main_fd, context.shell_pid)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

    #[cfg(not(unix))]
    let path = String::default();

    #[cfg(unix)]
    let program =
        teletypewriter::foreground_process_name(*context.main_fd, context.shell_pid);

    #[cfg(not(unix))]
    let program = String::default();

    Some(ContextTitleExtra { program, path })
}

// Possible options:

// - `TITLE`: terminal title via OSC sequences for setting terminal title
// - `PROGRAM`: (e.g `fish`, `zsh`, `bash`, `vim`, etc...)
// - `ABSOLUTE_PATH`: (e.g `/Users/rapha/Documents/a/rio`)
// - `CANONICAL_PATH`: (e.g `.../Documents/a/rio`, `~/Documents/a`)
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
                    #[cfg(unix)]
                    {
                        let program = teletypewriter::foreground_process_name(
                            *context.main_fd,
                            context.shell_pid,
                        );

                        new_template = new_template.replace(to_replace_str, &program);
                        matched = true;
                    }
                }
                "absolute_path" => {
                    {
                        let terminal = context.terminal.lock();
                        if let Some(current_directory) = &terminal.current_directory {
                            if let Ok(dir_str) =
                                current_directory.clone().into_os_string().into_string()
                            {
                                new_template =
                                    new_template.replace(to_replace_str, &dir_str);
                                matched = true;
                                continue;
                            }
                        };
                    }

                    #[cfg(unix)]
                    {
                        let path = teletypewriter::foreground_process_path(
                            *context.main_fd,
                            context.shell_pid,
                        )
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                        // In case it has a fallback and path is empty
                        // or
                        // In case is the last then we need to erase variables either way
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
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
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
            update_title("{{ columns }}x{{lines}}", &context),
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
        let context = create_mock_context(
            VoidListener {},
            WindowId::from(0),
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
            update_title("{{ title || title }}", &context),
            String::from("")
        );

        // let's modify title to actually be something
        {
            let mut term = context.terminal.lock();
            term.title = "Something".to_string();
        };

        assert_eq!(
            update_title("{{ title || columns }}", &context),
            String::from("Something")
        );

        assert_eq!(
            update_title("{{ columns || title }}", &context),
            String::from("66")
        );

        // let's modify current_directory to actually be something
        {
            let path = std::path::PathBuf::from("/tmp");
            let mut term = context.terminal.lock();
            term.current_directory = Some(path);
        };

        assert_eq!(
            update_title("{{ absolute_path || title }}", &context),
            String::from("/tmp"),
        );
    }
}
