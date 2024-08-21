use crate::constants;
use rio_backend::config::navigation::NavigationMode;

#[inline]
pub fn padding_top_from_config(navigation: &NavigationMode, num_tabs: usize) -> f32 {
    #[cfg(not(target_os = "macos"))]
    {
        if navigation == &NavigationMode::TopTab {
            if !(navigation.hide_single_tab && num_tabs > 1) {
                return constants::PADDING_Y_WITH_TAB_ON_TOP + config.padding_y[0];
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if navigation == &NavigationMode::NativeTab {
            return 0.0 + config.padding_y[0];
        }
    }

    constants::PADDING_Y + config.padding_y[0]
}

#[inline]
pub fn padding_bottom_from_config(navigation: &rio_backend::config::navigation::NavigationMode, num_tabs: usize) -> f32 {
    if navigation == &NavigationMode::BottomTab && !(navigation.hide_single_tab && num_tabs == 1) {
        config.fonts.size + config.padding_y[1]
    } else {
        0.0 + config.padding_y[1]
    }
}
