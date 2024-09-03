use crate::constants;
use rio_backend::config::navigation::{Navigation, NavigationMode};

#[inline]
pub fn padding_top_from_config(
    navigation: &Navigation,
    padding_y_top: f32,
    num_tabs: usize,
) -> f32 {
    let default_padding = constants::PADDING_Y + padding_y_top;

    #[cfg(not(target_os = "macos"))]
    {
        if navigation.hide_if_single && num_tabs == 1 {
            return default_padding;
        } else if navigation.mode == NavigationMode::TopTab {
            return constants::PADDING_Y_WITH_TAB_ON_TOP + padding_y_top;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if navigation.mode == NavigationMode::NativeTab {
            return 0.0 + padding_y_top;
        } else if navigation.hide_if_single && num_tabs == 1 {
            return default_padding;
        }
    }

    default_padding
}

#[inline]
pub fn padding_bottom_from_config(
    navigation: &Navigation,
    padding_y_bottom: f32,
    num_tabs: usize,
    is_search_active: bool,
) -> f32 {
    let default_padding = 0.0 + padding_y_bottom;

    if is_search_active {
        return padding_y_bottom + constants::PADDING_Y_BOTTOM_TABS;
    }

    if navigation.hide_if_single && num_tabs == 1 {
        return default_padding;
    }

    if navigation.mode == NavigationMode::BottomTab {
        return padding_y_bottom + constants::PADDING_Y_BOTTOM_TABS;
    }

    default_padding
}

#[inline]
pub fn terminal_dimensions(
    layout: &rio_backend::sugarloaf::layout::SugarloafLayout,
) -> teletypewriter::WinsizeBuilder {
    let width = layout.width - (layout.margin.x * 2.);
    let height = (layout.height - layout.margin.top_y) - layout.margin.bottom_y;
    teletypewriter::WinsizeBuilder {
        width: width as u16,
        height: height as u16,
        cols: layout.columns as u16,
        rows: layout.lines as u16,
    }
}
