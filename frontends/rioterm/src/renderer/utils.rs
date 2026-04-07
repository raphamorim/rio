use crate::constants;
use crate::layout::ContextDimension;
use crate::renderer::font_cache::{FontCache, FontCacheData};
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_backend::sugarloaf::font::FontLibrary;
use rio_backend::sugarloaf::{Content, SpanStyle};
use rio_window::window::Theme;

/// Add text to a Content builder with per-character font fallback.
/// Splits the text into spans grouped by resolved font_id.
#[inline]
pub fn add_span_with_fallback(
    builder: &mut Content,
    text: &str,
    base_style: SpanStyle,
    font_library: &FontLibrary,
    font_cache: &mut FontCache,
) {
    let font_attrs = base_style.font_attrs;
    let mut current_font_id: Option<usize> = None;
    let mut current_run = String::new();

    for ch in text.chars() {
        let font_id = if let Some(cached) = font_cache.get(&(ch, font_attrs)) {
            cached.font_id
        } else {
            let font_ctx = font_library.inner.read();
            let (fid, _) = font_ctx
                .find_best_font_match(ch, &base_style)
                .unwrap_or((0, false));
            font_cache.insert(
                (ch, font_attrs),
                FontCacheData {
                    font_id: fid,
                    width: 1.0,
                    is_pua: false,
                },
            );
            fid
        };

        if current_font_id == Some(font_id) {
            current_run.push(ch);
        } else {
            if !current_run.is_empty() {
                builder.add_span(
                    &current_run,
                    SpanStyle {
                        font_id: current_font_id.unwrap_or(0),
                        ..base_style
                    },
                );
                current_run.clear();
            }
            current_font_id = Some(font_id);
            current_run.push(ch);
        }
    }
    if !current_run.is_empty() {
        builder.add_span(
            &current_run,
            SpanStyle {
                font_id: current_font_id.unwrap_or(0),
                ..base_style
            },
        );
    }
}

#[inline]
pub fn padding_top_from_config(
    navigation: &Navigation,
    padding_y_top: f32,
    #[allow(unused)] num_tabs: usize,
    #[allow(unused)] macos_use_unified_titlebar: bool,
) -> f32 {
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
pub fn terminal_dimensions(layout: &ContextDimension) -> teletypewriter::WinsizeBuilder {
    let width = layout.width - layout.margin.left - layout.margin.right;
    let height = layout.height - layout.margin.top - layout.margin.bottom;
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
