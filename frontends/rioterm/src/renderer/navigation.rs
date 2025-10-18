use rio_backend::config::navigation::Navigation;
use std::collections::HashMap;

pub struct ScreenNavigation {
    pub navigation: Navigation,
    pub padding_y: [f32; 2],
    #[allow(dead_code)]
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
}
