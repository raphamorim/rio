use super::render_splash;
use crate::layout::ContextDimension;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let (_, win_h) = render_splash(sugarloaf, context_dimension);

    // "press enter" — bottom left
    let confirm_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(confirm_idx, false);
    sugarloaf.set_transient_text_font_size(confirm_idx, 14.0);

    if let Some(state) = sugarloaf.get_transient_text_mut(confirm_idx) {
        state
            .clear()
            .add_span("press enter", SpanStyle::default())
            .build();
    }

    sugarloaf.set_transient_position(confirm_idx, 20.0, win_h - 50.0);
    sugarloaf.set_transient_visibility(confirm_idx, true);

    // config path — bottom left below
    let config_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(config_idx, false);
    sugarloaf.set_transient_text_font_size(config_idx, 14.0);

    if let Some(state) = sugarloaf.get_transient_text_mut(config_idx) {
        let path = rio_backend::config::config_file_path();
        state
            .clear()
            .add_span(
                &path.display().to_string(),
                SpanStyle {
                    color: [0.5, 0.5, 0.5, 1.0],
                    ..SpanStyle::default()
                },
            )
            .build();
    }

    sugarloaf.set_transient_position(config_idx, 20.0, win_h - 30.0);
    sugarloaf.set_transient_visibility(config_idx, true);
}
