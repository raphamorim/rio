use crate::constants;
use crate::layout::ContextDimension;
use rio_backend::config::navigation::Navigation;
use rio_backend::config::Config;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};
use rio_window::window::Theme;

/// Add text to the currently selected text content with per-character
/// font fallback. Resolves each character against sugarloaf's glyph
/// cache, groups consecutive chars by resolved `font_id`, and emits
/// one span per group. The selected text id is whatever was last
/// passed to `Content::sel(...)` by the caller.
#[inline]
pub fn add_span_with_fallback(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    base_style: SpanStyle,
) {
    let font_attrs = base_style.font_attrs;

    // First walk: resolve every char against sugarloaf's font cache
    // and group into runs by font_id. We can't push to `content` yet
    // because `resolve_glyph` borrows sugarloaf mutably to fill the
    // cache.
    let mut runs: Vec<(usize, String)> = Vec::new();
    let mut current_font_id: Option<usize> = None;
    let mut current_run = String::new();

    for ch in text.chars() {
        let glyph = sugarloaf.resolve_glyph(ch, font_attrs);
        if current_font_id == Some(glyph.font_id) {
            current_run.push(ch);
        } else {
            if !current_run.is_empty() {
                runs.push((
                    current_font_id.unwrap_or(0),
                    std::mem::take(&mut current_run),
                ));
            }
            current_font_id = Some(glyph.font_id);
            current_run.push(ch);
        }
    }
    if !current_run.is_empty() {
        runs.push((current_font_id.unwrap_or(0), current_run));
    }

    // Second walk: emit the runs. Now we can take `&mut Content`
    // because all the cache fills are done.
    let content = sugarloaf.content();
    for (font_id, run) in runs {
        content.add_span(
            &run,
            SpanStyle {
                font_id,
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
