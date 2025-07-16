use crate::constants::*;
use crate::context::title::ContextTitle;
use rio_backend::config::colors::Colors;
use rio_backend::config::navigation::{Navigation, NavigationMode};
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};
use rustc_hash::FxHashMap;
use std::collections::HashMap;

pub struct ScreenNavigation {
    pub navigation: Navigation,
    pub padding_y: [f32; 2],
    color_automation: HashMap<String, HashMap<String, [f32; 4]>>,
}

impl ScreenNavigation {
    pub fn new(
        navigation: Navigation,
        color_automation: HashMap<String, HashMap<String, [f32; 4]>>,
        padding_y: [f32; 2],
    ) -> ScreenNavigation {
        ScreenNavigation {
            navigation,
            color_automation,
            padding_y,
        }
    }

    #[inline]
    pub fn build_objects(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        colors: &Colors,
        context_manager: &crate::context::ContextManager<rio_backend::event::EventProxy>,
        is_search_active: bool,
        objects: &mut Vec<Object>,
    ) {
        // When search is active then BottomTab should not be rendered
        if is_search_active && self.navigation.mode == NavigationMode::BottomTab {
            return;
        }

        let current = context_manager.current_index();
        let len = context_manager.len();

        let titles = &context_manager.titles.titles;

        match self.navigation.mode {
            #[cfg(target_os = "macos")]
            NavigationMode::NativeTab => {}
            NavigationMode::Bookmark => self.bookmark(
                objects,
                titles,
                colors,
                len,
                current,
                self.navigation.hide_if_single,
                dimensions,
            ),
            NavigationMode::TopTab => {
                let position_y = 0.0;
                self.tab(
                    sugarloaf,
                    objects,
                    titles,
                    colors,
                    len,
                    current,
                    position_y,
                    self.navigation.hide_if_single,
                    dimensions,
                );
            }
            NavigationMode::BottomTab => {
                let (_, height, scale) = dimensions;
                let position_y = (height / scale) - PADDING_Y_BOTTOM_TABS;
                self.tab(
                    sugarloaf,
                    objects,
                    titles,
                    colors,
                    len,
                    current,
                    position_y,
                    self.navigation.hide_if_single,
                    dimensions,
                );
            }
            // Minimal simply does not do anything
            NavigationMode::Plain => {}
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn bookmark(
        &mut self,
        objects: &mut Vec<Object>,
        titles: &FxHashMap<usize, ContextTitle>,
        colors: &Colors,
        len: usize,
        current: usize,
        hide_if_single: bool,
        dimensions: (f32, f32, f32),
    ) {
        if hide_if_single && len <= 1 {
            return;
        }

        let (width, _, scale) = dimensions;

        let mut initial_position = (width / scale) - PADDING_X_COLLAPSED_TABS;
        let position_modifier = 20.;
        for i in (0..len).rev() {
            let mut color = colors.tabs;
            let mut size = INACTIVE_TAB_WIDTH_SIZE;
            if i == current {
                color = colors.tabs_active_highlight;
                size = ACTIVE_TAB_WIDTH_SIZE;
            }

            if let Some(title) = titles.get(&i) {
                if !self.color_automation.is_empty() {
                    if let Some(extra) = &title.extra {
                        if let Some(color_overwrite) = get_color_overwrite(
                            &self.color_automation,
                            &extra.program,
                            &extra.path,
                        ) {
                            color = *color_overwrite;
                        }
                    }
                }
            }

            let renderable = Quad {
                position: [initial_position, 0.0],
                color,
                size: [15.0, size],
                ..Quad::default()
            };
            initial_position -= position_modifier;
            objects.push(Object::Quad(renderable));
        }
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn tab(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        objects: &mut Vec<Object>,
        titles: &FxHashMap<usize, ContextTitle>,
        colors: &Colors,
        len: usize,
        current: usize,
        position_y: f32,
        hide_if_single: bool,
        dimensions: (f32, f32, f32),
    ) {
        if hide_if_single && len <= 1 {
            return;
        }

        let (width, _, scale) = dimensions;

        let mut initial_position_x = 0.;

        let renderable = Quad {
            position: [initial_position_x, position_y],
            color: colors.bar,
            size: [width, PADDING_Y_BOTTOM_TABS],
            ..Quad::default()
        };

        objects.push(Object::Quad(renderable));

        let iter = 0..len;
        let mut tabs = Vec::from_iter(iter);

        let max_tab_width = 140.;
        let screen_limit = ((width / scale) / max_tab_width).floor() as usize;
        if len > screen_limit && current > screen_limit {
            tabs = Vec::from_iter(current - screen_limit..len);
        }

        for i in tabs {
            let mut background_color = colors.bar;
            let mut foreground_color = colors.tabs_foreground;

            let is_current = i == current;
            if is_current {
                foreground_color = colors.tabs_active_foreground;
                background_color = colors.tabs_active;
            }

            let mut name = String::from("tab");
            if let Some(title) = titles.get(&i) {
                name = title.content.to_owned();

                if !self.color_automation.is_empty() {
                    if let Some(extra) = &title.extra {
                        if let Some(color_overwrite) = get_color_overwrite(
                            &self.color_automation,
                            &extra.program,
                            &extra.path,
                        ) {
                            foreground_color = colors.tabs;
                            background_color = *color_overwrite;
                        }
                    }
                }
            }

            let name_modifier = 90.;
            if name.len() >= 14 {
                name = name[0..14].to_string();
            }

            objects.push(Object::Quad(Quad {
                position: [initial_position_x, position_y],
                color: background_color,
                size: [125., PADDING_Y_BOTTOM_TABS],
                ..Quad::default()
            }));

            if is_current {
                // TopBar case should render on bottom
                let position = if position_y == 0.0 {
                    PADDING_Y_BOTTOM_TABS - (PADDING_Y_BOTTOM_TABS / 10.)
                } else {
                    position_y
                };

                objects.push(Object::Quad(Quad {
                    position: [initial_position_x, position],
                    color: colors.tabs_active_highlight,
                    size: [125., PADDING_Y_BOTTOM_TABS / 10.],
                    ..Quad::default()
                }));
            }

            let text = if is_current {
                format!("▲ {name}")
            } else {
                format!("{}.{name}", i + 1)
            };

            let tab = sugarloaf.create_temp_rich_text();
            sugarloaf.set_rich_text_font_size(&tab, 14.);
            let content = sugarloaf.content();

            let tab_line = content.sel(tab);
            tab_line
                .clear()
                .new_line()
                .add_text(
                    &text,
                    FragmentStyle {
                        color: foreground_color,
                        ..FragmentStyle::default()
                    },
                )
                .build();

            objects.push(Object::RichText(RichText {
                id: tab,
                position: [initial_position_x + 4., position_y],
                lines: None,
            }));

            initial_position_x += name_modifier + 40.;
        }
    }
}

#[inline]
fn get_color_overwrite<'a>(
    color_automation: &'a HashMap<String, HashMap<String, [f32; 4]>>,
    program: &str,
    path: &str,
) -> Option<&'a [f32; 4]> {
    color_automation
        .get(program)
        .and_then(|m| m.get(path).or_else(|| m.get("")))
        .or_else(|| color_automation.get("").and_then(|m| m.get(path)))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::renderer::navigation::get_color_overwrite;

    #[test]
    fn test_get_color_overwrite() {
        let program = "nvim";
        let path = "/home/";

        let program_and_path = [0.0, 0.0, 0.0, 0.0];
        let program_only = [1.1, 1.1, 1.1, 1.1];
        let path_only = [2.2, 2.2, 2.2, 2.2];
        let neither = [3.3, 3.3, 3.3, 3.3];

        let color_automation = HashMap::from([
            (
                program.to_owned(),
                HashMap::from([
                    (path.to_owned(), program_and_path),
                    (String::new(), program_only),
                ]),
            ),
            (
                String::new(),
                HashMap::from([(path.to_owned(), path_only), (String::new(), neither)]),
            ),
        ]);

        let program_and_path_result =
            get_color_overwrite(&color_automation, program, path)
                .expect("it to return a color");

        assert_eq!(&program_and_path, program_and_path_result);

        let program_only_result = get_color_overwrite(&color_automation, program, "")
            .expect("it to return a color");

        assert_eq!(&program_only, program_only_result);

        let path_only_result = get_color_overwrite(&color_automation, "", path)
            .expect("it to return a color");

        assert_eq!(&path_only, path_only_result);

        let neither_result =
            get_color_overwrite(&color_automation, "", "").expect("it to return a color");

        assert_eq!(&neither, neither_result);
    }
}
