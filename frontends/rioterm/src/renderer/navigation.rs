use rio_backend::config::colors::Colors;
use rio_backend::config::navigation::{Navigation, NavigationMode};
use rio_backend::sugarloaf::Object;
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
        _sugarloaf: &mut rio_backend::sugarloaf::Sugarloaf,
        _dimensions: (f32, f32, f32),
        _colors: &Colors,
        _context_manager: &crate::context::ContextManager<rio_backend::event::EventProxy>,
        is_search_active: bool,
        _objects: &mut Vec<Object>,
    ) {
        // When search is active, navigation should not be rendered
        if is_search_active {
            return;
        }

        match self.navigation.mode {
            #[cfg(target_os = "macos")]
            NavigationMode::NativeTab => {}
            // Rio and Plain modes do not render any navigation
            NavigationMode::Rio | NavigationMode::Plain => {}
        }
    }
}
