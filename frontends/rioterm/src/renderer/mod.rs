use crate::constants;
use rio_backend::config::navigation::{Navigation, NavigationMode};

#[inline]
pub fn padding_top_from_config(
    navigation: &Navigation,
    padding_y_top: f32,
    num_tabs: usize,
) -> f32 {
    let default_padding = constants::PADDING_Y + padding_y_top;

    if navigation.hide_if_single && num_tabs == 1 {
        return default_padding;
    }

    #[cfg(not(target_os = "macos"))]
    {
        if navigation.mode == NavigationMode::TopTab {
            return constants::PADDING_Y_WITH_TAB_ON_TOP + padding_y_top;
        }
    }

    #[cfg(target_os = "macos")]
    {
        if navigation.mode == NavigationMode::NativeTab {
            return 0.0 + padding_y_top;
        }
    }

    default_padding
}

#[inline]
pub fn padding_bottom_from_config(
    navigation: &Navigation,
    padding_y_bottom: f32,
    num_tabs: usize,
) -> f32 {
    let default_padding = 0.0 + padding_y_bottom;
    if navigation.hide_if_single && num_tabs == 1 {
        return default_padding;
    }

    if navigation.mode == NavigationMode::BottomTab {
        return padding_y_bottom + constants::PADDING_Y_BOTTOM_TABS;
    }

    default_padding
}

pub fn draw_search_bar(
    dimensions: (f32, f32),
    scale: f32,
    content: &String,
) -> (Vec<Rect>, Text) {
    let (width, height) = dimensions;
    let position_y = (height / scale) - 40.;

    let rects = vec![Rect {
        position: [0.0, position_y],
        color: [0.0, 0.1, 0.1, 1.0],
        size: [width * (scale + 1.0), 22.0],
    }];

    let text = Text::new((4., position_y), content.to_string(), 14., [0., 0., 0., 1.]);

    (rects, text)
}
