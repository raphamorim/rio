use crate::constants;
use crate::context::grid::ContextDimension;
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_window::window::Theme;

#[inline]
pub fn padding_top_from_config(
    navigation: &Navigation,
    padding_y_top: f32,
    _num_tabs: usize,
    #[allow(unused)] macos_use_unified_titlebar: bool,
) -> f32 {
    // When navigation is enabled (Tab mode), start content below island
    if navigation.is_enabled() {
        use crate::renderer::island::ISLAND_HEIGHT;
        return ISLAND_HEIGHT + padding_y_top;
    }

    let default_padding = constants::PADDING_Y + padding_y_top;

    #[cfg(target_os = "macos")]
    {
        use rio_backend::config::navigation::NavigationMode;
        if navigation.mode == NavigationMode::NativeTab {
            let additional = if macos_use_unified_titlebar {
                constants::ADDITIONAL_PADDING_Y_ON_UNIFIED_TITLEBAR
            } else {
                0.0
            };
            return additional + padding_y_top;
        }
    }

    default_padding
}

#[inline]
pub fn padding_bottom_from_config(
    _navigation: &Navigation,
    padding_y_bottom: f32,
    _num_tabs: usize,
    is_search_active: bool,
) -> f32 {
    let default_padding = 0.0 + padding_y_bottom;

    if is_search_active {
        return padding_y_bottom + constants::PADDING_Y_BOTTOM_TABS;
    }

    default_padding
}

#[inline]
pub fn terminal_dimensions(layout: &ContextDimension) -> teletypewriter::WinsizeBuilder {
    let width = layout.width - (layout.margin.x * 2.);
    let height = (layout.height - layout.margin.top_y) - layout.margin.bottom_y;
    teletypewriter::WinsizeBuilder {
        width: width as u16,
        height: height as u16,
        cols: layout.columns as u16,
        rows: layout.lines as u16,
    }
}

#[inline]
pub fn update_colors_based_on_theme(config: &mut Config, theme_opt: Option<Theme>) {
    if let Some(theme) = theme_opt {
        if let Some(adaptive_colors) = &config.adaptive_colors {
            match theme {
                Theme::Light => {
                    if let Some(light_colors) = adaptive_colors.light {
                        config.colors = light_colors;
                    }
                }
                Theme::Dark => {
                    if let Some(darkcolors) = adaptive_colors.dark {
                        config.colors = darkcolors;
                    }
                }
            }
        }
    }
}
