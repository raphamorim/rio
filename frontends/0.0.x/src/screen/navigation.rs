use crate::screen::constants::*;
use rio_backend::config::navigation::NavigationMode;
use rio_backend::sugarloaf::components::rect::Rect;
use rio_backend::sugarloaf::font::FONT_ID_BUILTIN;
use std::collections::HashMap;

pub struct Text {
    pub position: (f32, f32),
    pub content: String,
    pub font_id: usize,
    pub font_size: f32,
    pub color: [f32; 4],
}

impl Text {
    #[inline]
    pub fn new(
        position: (f32, f32),
        content: String,
        font_id: usize,
        font_size: f32,
        color: [f32; 4],
    ) -> Self {
        Text {
            position,
            content,
            font_id,
            font_size,
            color,
        }
    }
}

pub struct ScreenNavigationColors {
    foreground: [f32; 4],
    active: [f32; 4],
    inactive: [f32; 4],
}

pub struct ScreenNavigation {
    pub mode: NavigationMode,
    pub rects: Vec<Rect>,
    pub texts: Vec<Text>,
    keys: String,
    current: usize,
    colors: ScreenNavigationColors,
    width: f32,
    height: f32,
    scale: f32,
    color_automation: HashMap<String, HashMap<String, [f32; 4]>>,
}

impl ScreenNavigation {
    pub fn new(
        mode: NavigationMode,
        colors: [[f32; 4]; 3],
        color_automation: HashMap<String, HashMap<String, [f32; 4]>>,
        width: f32,
        height: f32,
        scale: f32,
    ) -> ScreenNavigation {
        let colors = {
            ScreenNavigationColors {
                inactive: colors[0],
                active: colors[1],
                foreground: colors[2],
            }
        };

        ScreenNavigation {
            mode,
            rects: vec![],
            texts: vec![],
            keys: String::from(""),
            color_automation,
            current: 0,
            colors,
            width,
            height,
            scale,
        }
    }

    #[inline]
    pub fn content(
        &mut self,
        dimensions: (f32, f32),
        scale: f32,
        keys: &str,
        titles: &HashMap<usize, [String; 3]>,
        current: usize,
        len: usize,
    ) {
        let mut has_changes = false;

        if dimensions.0 != self.width {
            self.width = dimensions.0;
            has_changes = true;
        }

        if dimensions.1 != self.height {
            self.height = dimensions.1;
            has_changes = true;
        }

        if scale != self.scale {
            self.scale = scale;
            has_changes = true;
        }

        if keys != self.keys {
            self.keys = keys.to_string();
            has_changes = true;
        }

        if current != self.current {
            self.current = current;
            has_changes = true;
        }

        if !has_changes {
            return;
        }

        self.rects = vec![];
        self.texts = vec![];

        match self.mode {
            #[cfg(target_os = "macos")]
            NavigationMode::NativeTab => {}
            NavigationMode::CollapsedTab => self.collapsed_tab(titles, len),
            #[cfg(not(windows))]
            NavigationMode::Breadcrumb => self.breadcrumb(titles, len),
            NavigationMode::TopTab => {
                let position_y = 0.0;
                self.tab(titles, len, position_y, 11.);
            }
            NavigationMode::BottomTab => {
                let position_y = (self.height / self.scale) - 20.;
                self.tab(titles, len, position_y, 9.);
            }
            // Minimal simply does not do anything
            NavigationMode::Plain => {}
        }
    }

    #[inline]
    pub fn collapsed_tab(&mut self, titles: &HashMap<usize, [String; 3]>, len: usize) {
        if len <= 1 {
            return;
        }

        let mut initial_position = (self.width / self.scale) - PADDING_X_COLLAPSED_TABS;
        let position_modifier = 20.;
        for i in (0..len).rev() {
            let mut color = self.colors.inactive;
            let mut size = INACTIVE_TAB_WIDTH_SIZE;
            if i == self.current {
                color = self.colors.active;
                size = ACTIVE_TAB_WIDTH_SIZE;
            }

            if let Some(name_idx) = titles.get(&i) {
                if let Some(color_overwrite) = get_color_overwrite(
                    &self.color_automation,
                    &name_idx[0],
                    &name_idx[2],
                ) {
                    color = *color_overwrite;
                }
            }

            let renderable = Rect {
                position: [initial_position, 0.0],
                color,
                size: [30.0, size],
            };
            initial_position -= position_modifier;
            self.rects.push(renderable);
        }
    }

    #[inline]
    pub fn breadcrumb(&mut self, titles: &HashMap<usize, [String; 3]>, len: usize) {
        let mut initial_position = (self.width / self.scale) - 100.;
        let position_modifier = 80.;
        let mut min_view = 9;

        if (self.width / self.scale) <= 440. {
            min_view = 1;
        }

        let current_index = self.current;
        let mut bg_color = self.colors.active;
        let mut fg_color = self.colors.inactive;
        let mut icon_color = self.colors.active;

        let mut main_name = String::from("tab");
        if let Some(main_name_idx) = titles.get(&current_index) {
            main_name = main_name_idx[0].to_string();

            if let Some(color_overwrite) = get_color_overwrite(
                &self.color_automation,
                &main_name_idx[0],
                &main_name_idx[2],
            ) {
                fg_color = self.colors.inactive;
                bg_color = *color_overwrite;
                icon_color = bg_color;
            }
        }

        if main_name.len() > 12 {
            main_name = main_name[0..12].to_string();
        }

        let renderable = Rect {
            position: [initial_position, 0.0],
            color: bg_color,
            size: [200., 26.0],
        };

        self.texts.push(Text::new(
            (initial_position - 12., 14.5),
            "".to_string(),
            FONT_ID_BUILTIN,
            23.,
            icon_color,
        ));

        self.texts.push(Text::new(
            (initial_position + 4., 13.0),
            format!("{}.{}", current_index + 1, main_name),
            FONT_ID_BUILTIN,
            14.,
            fg_color,
        ));

        initial_position -= position_modifier;
        self.rects.push(renderable);

        if len <= 1 {
            return;
        }

        let mut iterator = current_index;
        if len - 1 == iterator {
            iterator = 0;
        } else {
            iterator += 1;
        }

        if min_view == 1 {
            if len > 1 {
                self.texts.push(Text::new(
                    (initial_position + 36., 13.0),
                    format!("+ {}", len - 1),
                    FONT_ID_BUILTIN,
                    13.,
                    self.colors.foreground,
                ));
            }
        } else {
            let mut rendered = len - 1;
            while rendered > 0 {
                if iterator == self.current {
                    continue;
                }

                if initial_position <= 120.0 {
                    self.texts.push(Text::new(
                        (initial_position + 36., 13.0),
                        format!("+ {}", rendered),
                        FONT_ID_BUILTIN,
                        13.,
                        self.colors.foreground,
                    ));
                    break;
                }

                let mut bg_color = self.colors.inactive;
                let mut fg_color = self.colors.active;
                let mut icon_color = self.colors.inactive;

                let mut name = String::from("tab");
                if let Some(name_idx) = titles.get(&iterator) {
                    name = name_idx[0].to_string();

                    if let Some(color_overwrite) = get_color_overwrite(
                        &self.color_automation,
                        &name_idx[0],
                        &name_idx[2],
                    ) {
                        fg_color = self.colors.inactive;
                        bg_color = *color_overwrite;
                        icon_color = bg_color;
                    }
                }

                if name.len() > 7 {
                    name = name[0..7].to_string();
                }

                let renderable_item = Rect {
                    position: [initial_position, 0.0],
                    color: bg_color,
                    size: [160., 26.],
                };

                self.texts.push(Text::new(
                    (initial_position - 12., 15.0),
                    "".to_string(),
                    FONT_ID_BUILTIN,
                    22.,
                    icon_color,
                ));

                self.texts.push(Text::new(
                    (initial_position + 4., 13.0),
                    format!("{}.{}", iterator + 1, name),
                    FONT_ID_BUILTIN,
                    14.,
                    fg_color,
                ));

                initial_position -= position_modifier;
                self.rects.push(renderable_item);

                if len - 1 == iterator {
                    iterator = 0;
                } else {
                    iterator += 1;
                }

                rendered -= 1;
            }
        }
    }

    #[inline]
    pub fn tab(
        &mut self,
        titles: &HashMap<usize, [String; 3]>,
        len: usize,
        position_y: f32,
        text_pos_mod: f32,
    ) {
        let mut initial_position_x = 0.;

        let renderable = Rect {
            position: [initial_position_x, position_y],
            color: self.colors.inactive,
            size: [self.width * (self.scale + 1.0), 22.0],
        };

        self.rects.push(renderable);

        let iter = 0..len;
        let mut tabs = Vec::from_iter(iter);

        let max_tab_width = 150.;
        let screen_limit = ((self.width / self.scale) / max_tab_width).floor() as usize;
        if len > screen_limit && self.current > screen_limit {
            tabs = Vec::from_iter(self.current - screen_limit..len);
        }

        for i in tabs {
            let mut background_color = self.colors.inactive;
            let mut foreground_color = self.colors.active;

            if i == self.current {
                foreground_color = self.colors.inactive;
                background_color = self.colors.active;
            }

            let mut name = String::from("tab");
            if let Some(name_idx) = titles.get(&i) {
                if !name_idx[1].is_empty() {
                    name = name_idx[1].to_string();
                } else {
                    name = name_idx[0].to_string();
                }

                if let Some(color_overwrite) = get_color_overwrite(
                    &self.color_automation,
                    &name_idx[0],
                    &name_idx[2],
                ) {
                    foreground_color = self.colors.inactive;
                    background_color = *color_overwrite;
                }
            }

            let mut name_modifier = 100.;

            if name.len() >= 20 {
                name = name[0..20].to_string();
                name_modifier += 80.;
            } else if name.len() >= 15 {
                name = name[0..15].to_string();
                name_modifier += 40.;
            } else if name.len() >= 10 {
                name = name[0..10].to_string();
                name_modifier += 20.;
            }

            let renderable_item = Rect {
                position: [initial_position_x, position_y],
                color: background_color,
                size: [120. + name_modifier + 30., 22.],
            };

            self.texts.push(Text::new(
                (initial_position_x + 4., position_y + text_pos_mod),
                format!("{}.{}", i + 1, name),
                FONT_ID_BUILTIN,
                14.,
                foreground_color,
            ));

            initial_position_x += name_modifier;
            self.rects.push(renderable_item);
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

    use crate::screen::navigation::get_color_overwrite;

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
