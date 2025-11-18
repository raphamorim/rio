use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    heading_content: &str,
    confirm_content: &str,
    quit_content: &str,
) {
    let layout = sugarloaf.window_size();

    // Render rectangles directly
    sugarloaf.rect(
        None,
        0.0,
        0.0,
        layout.width / context_dimension.dimension.scale,
        layout.height,
        [0.0, 0.0, 0.0, 0.5],
        0.0,
    );
    sugarloaf.rect(None, 128.0, 256.0, 350.0, 150.0, [0.0, 0.0, 0.0, 1.0], 0.0);
    sugarloaf.rect(
        None,
        128.0,
        320.0,
        106.0,
        36.0,
        [0.133, 0.141, 0.176, 1.0],
        0.0,
    );
    sugarloaf.rect(
        None,
        240.0,
        320.0,
        106.0,
        36.0,
        [0.133, 0.141, 0.176, 1.0],
        0.0,
    );

    // Create transient text elements (rendered once then cleaned up)
    let heading_idx = sugarloaf.text(None);
    let confirm_idx = sugarloaf.text(None);
    let quit_idx = sugarloaf.text(None);

    // Use proportional text rendering (not monospace grid)
    sugarloaf.set_transient_use_grid_cell_size(heading_idx, false);
    sugarloaf.set_transient_use_grid_cell_size(confirm_idx, false);
    sugarloaf.set_transient_use_grid_cell_size(quit_idx, false);

    sugarloaf.set_transient_text_font_size(heading_idx, 32.0);
    sugarloaf.set_transient_text_font_size(confirm_idx, 20.0);
    sugarloaf.set_transient_text_font_size(quit_idx, 20.0);

    // Add text content to transient elements
    if let Some(heading_state) = sugarloaf.get_transient_text_mut(heading_idx) {
        heading_state
            .clear()
            .add_span(heading_content, SpanStyle::default())
            .build();
    }

    if let Some(confirm_state) = sugarloaf.get_transient_text_mut(confirm_idx) {
        confirm_state
            .clear()
            .add_span(confirm_content, SpanStyle::default())
            .build();
    }

    if let Some(quit_state) = sugarloaf.get_transient_text_mut(quit_idx) {
        quit_state
            .clear()
            .add_span(quit_content, SpanStyle::default())
            .build();
    }

    // Show rich texts at specific positions
    sugarloaf.set_transient_position(heading_idx, 150.0, 270.0);
    sugarloaf.set_transient_visibility(heading_idx, true);

    sugarloaf.set_transient_position(confirm_idx, 150.0, 330.0);
    sugarloaf.set_transient_visibility(confirm_idx, true);

    sugarloaf.set_transient_position(quit_idx, 268.0, 330.0);
    sugarloaf.set_transient_visibility(quit_idx, true);
}
