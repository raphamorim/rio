use crate::constants;

#[inline]
pub fn padding_top_from_config(config: &rio_backend::config::Config) -> f32 {
    #[cfg(not(target_os = "macos"))]
    {
        if config.navigation.is_placed_on_top() {
            return constants::PADDING_Y_WITH_TAB_ON_TOP + config.padding_y[0];
        }
    }

    #[cfg(target_os = "macos")]
    {
        if config.navigation.is_native() {
            return 0.0 + config.padding_y[0];
        }
    }

    constants::PADDING_Y + config.padding_y[0]
}

#[inline]
pub fn padding_bottom_from_config(config: &rio_backend::config::Config) -> f32 {
    if config.navigation.is_placed_on_bottom() {
        config.fonts.size + config.padding_y[1]
    } else {
        0.0 + config.padding_y[1]
    }
}
