use crate::constants;
use crate::layout::ContextDimension;
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_window::window::Theme;

#[inline]
pub fn padding_top_from_config(
    navigation: &Navigation,
    padding_y_top: f32,
    #[allow(unused)] num_tabs: usize,
    #[allow(unused)] macos_use_unified_titlebar: bool,
    unified_titlebar: bool,
) -> f32 {
    // Under a Wayland unified title bar the strip is the window's title
    // bar, with the compositor's buttons over it: it is kept whatever the
    // navigation mode and the number of tabs.
    if unified_titlebar {
        use crate::renderer::island::ISLAND_HEIGHT;
        return ISLAND_HEIGHT + padding_y_top;
    }

    // When navigation is enabled (Tab mode), start content below island
    if navigation.is_enabled() {
        // On Linux/Windows, if hide_if_single is true and there's only one tab,
        // the island is hidden so render from 0 + configured margin
        #[cfg(not(target_os = "macos"))]
        if navigation.hide_if_single && num_tabs <= 1 {
            return constants::PADDING_Y + padding_y_top;
        }

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
pub fn terminal_dimensions(layout: &ContextDimension) -> rio_backend::event::WindowSize {
    let width = layout.width - layout.margin.left - layout.margin.right;
    let height = layout.height - layout.margin.top - layout.margin.bottom;
    rio_backend::event::WindowSize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::island::ISLAND_HEIGHT;
    use rio_backend::config::navigation::NavigationMode;

    #[test]
    fn a_unified_titlebar_keeps_the_strip_band() {
        let plain = Navigation {
            mode: NavigationMode::Plain,
            ..Navigation::default()
        };
        let single = Navigation {
            hide_if_single: true,
            ..Navigation::default()
        };
        for navigation in [plain, single] {
            assert_eq!(
                padding_top_from_config(&navigation, 2.0, 1, false, true),
                ISLAND_HEIGHT + 2.0
            );
        }
    }
}
