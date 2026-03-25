use super::render_splash;
use crate::layout::ContextDimension;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    heading_content: &str,
    confirm_content: &str,
    quit_content: &str,
) {
    let (_, win_h) = render_splash(sugarloaf, context_dimension);

    let gray = [0.5, 0.5, 0.5, 1.0];

    // "want to quit?" — bottom left
    let heading_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(heading_idx, false);
    sugarloaf.set_transient_text_font_size(heading_idx, 14.0);

    if let Some(state) = sugarloaf.get_transient_text_mut(heading_idx) {
        state
            .clear()
            .add_span(heading_content, SpanStyle::default())
            .build();
    }

    sugarloaf.set_transient_position(heading_idx, 20.0, win_h - 50.0);
    sugarloaf.set_transient_visibility(heading_idx, true);

    // "yes (y)  /  no (n)" — bottom left below
    let options_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(options_idx, false);
    sugarloaf.set_transient_text_font_size(options_idx, 14.0);

    let options = format!("{}  /  {}", confirm_content, quit_content);
    if let Some(state) = sugarloaf.get_transient_text_mut(options_idx) {
        state
            .clear()
            .add_span(
                &options,
                SpanStyle {
                    color: gray,
                    ..SpanStyle::default()
                },
            )
            .build();
    }

    sugarloaf.set_transient_position(options_idx, 20.0, win_h - 30.0);
    sugarloaf.set_transient_visibility(options_idx, true);
}
