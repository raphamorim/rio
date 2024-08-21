use crate::constants::*;
use rio_backend::config::navigation::{Navigation, NavigationMode};
use rio_backend::sugarloaf::{Object, Rect, Text};
use std::collections::HashMap;

pub struct ScreenNavigationColors {
    #[allow(dead_code)]
    foreground: [f32; 4],
    bar: [f32; 4],
    active: [f32; 4],
    inactive: [f32; 4],
}

pub struct ScreenNavigation {
    pub navigation: Navigation,
    pub objects: Vec<Object>,
    keys: String,
    current: usize,
    colors: ScreenNavigationColors,
    width: f32,
    height: f32,
    scale: f32,
    pub padding_y: [f32; 2],
    color_automation: HashMap<String, HashMap<String, [f32; 4]>>,
}

impl ScreenNavigation {
    pub fn new(
        navigation: Navigation,
        colors: [[f32; 4]; 4],
        color_automation: HashMap<String, HashMap<String, [f32; 4]>>,
        padding_y: [f32; 2],
    ) -> ScreenNavigation {
        let colors = {
            ScreenNavigationColors {
                inactive: colors[0],
                bar: colors[1],
                active: colors[2],
                foreground: colors[3],
            }
        };

        ScreenNavigation {
            navigation,
            objects: vec![],
            keys: String::from(""),
            color_automation,
            current: 0,
            colors,
            padding_y,
            width: 0.0,
            height: 0.0,
            scale: 0.0,
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

        self.objects.clear();

        match self.navigation.mode {
            #[cfg(target_os = "macos")]
            NavigationMode::NativeTab => {}
            NavigationMode::CollapsedTab => {
                self.collapsed_tab(titles, len, self.navigation.hide_if_single)
            }
            NavigationMode::TopTab => {
                let position_y = 0.0;
                self.tab(titles, len, position_y, self.navigation.hide_if_single);
            }
            NavigationMode::BottomTab => {
                let position_y = (self.height / self.scale) - PADDING_Y_BOTTOM_TABS;
                self.tab(titles, len, position_y, self.navigation.hide_if_single);
            }
            // Minimal simply does not do anything
            NavigationMode::Plain => {}
        }
    }

    #[inline]
    pub fn collapsed_tab(
        &mut self,
        titles: &HashMap<usize, [String; 3]>,
        len: usize,
        hide_if_single: bool,
    ) {
        if hide_if_single && len <= 1 {
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
            self.objects.push(Object::Rect(renderable));
        }
    }

    #[inline]
    pub fn tab(
        &mut self,
        titles: &HashMap<usize, [String; 3]>,
        len: usize,
        position_y: f32,
        hide_if_single: bool,
    ) {
        if hide_if_single && len <= 1 {
            return;
        }

        let mut initial_position_x = 0.;

        let renderable = Rect {
            position: [initial_position_x, position_y],
            color: self.colors.bar,
            size: [
                (self.width + PADDING_Y_BOTTOM_TABS) * self.scale,
                PADDING_Y_BOTTOM_TABS,
            ],
        };

        self.objects.push(Object::Rect(renderable));

        let iter = 0..len;
        let mut tabs = Vec::from_iter(iter);

        let max_tab_width = 90.;
        let screen_limit = ((self.width / self.scale) / max_tab_width).floor() as usize;
        if len > screen_limit && self.current > screen_limit {
            tabs = Vec::from_iter(self.current - screen_limit..len);
        }

        let text_pos_mod = 11.;
        for i in tabs {
            let mut background_color = self.colors.bar;
            let mut foreground_color = self.colors.active;

            let is_current = i == self.current;
            if is_current {
                foreground_color = self.colors.foreground;
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

            let name_modifier = 80.;
            if name.len() >= 10 {
                name = name[0..10].to_string();
            }

            self.objects.push(Object::Rect(Rect {
                position: [initial_position_x, position_y],
                color: background_color,
                size: [180., PADDING_Y_BOTTOM_TABS],
            }));

            if is_current {
                let mut modified_color = background_color;
                modified_color[0] = modified_color[0].round();
                modified_color[1] = modified_color[1].round();
                modified_color[2] = modified_color[3].round();
                self.objects.push(Object::Rect(Rect {
                    position: [initial_position_x, position_y],
                    color: modified_color,
                    size: [180., PADDING_Y_BOTTOM_TABS / 10.],
                }));
            }

            let text = if is_current {
                format!("▲ {}", name)
            } else {
                format!("{}.{}", i + 1, name)
            };

            self.objects.push(Object::Text(Text::single_line(
                (initial_position_x + 4., position_y + text_pos_mod),
                text,
                14.,
                foreground_color,
            )));

            initial_position_x += name_modifier + 10.;
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

    use crate::state::navigation::get_color_overwrite;

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
