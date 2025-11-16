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
    sugarloaf.rect(None, 128.0, 320.0, 106.0, 36.0, [0.133, 0.141, 0.176, 1.0], 0.0);
    sugarloaf.rect(None, 240.0, 320.0, 106.0, 36.0, [0.133, 0.141, 0.176, 1.0], 0.0);

    // Use simple IDs for transient UI elements (not cached)
    let heading_id = 1003;
    let confirm_id = 1004;
    let quit_id = 1005;

    let _ = sugarloaf.text(heading_id);
    let _ = sugarloaf.text(confirm_id);
    let _ = sugarloaf.text(quit_id);

    sugarloaf.set_text_font_size(&heading_id, 32.0);
    sugarloaf.set_text_font_size(&confirm_id, 20.0);
    sugarloaf.set_text_font_size(&quit_id, 20.0);

    let content = sugarloaf.content();

    content
        .sel(heading_id)
        .clear()
        .add_text(heading_content, SpanStyle::default())
        .build();

    content
        .sel(confirm_id)
        .clear()
        .add_text(confirm_content, SpanStyle::default())
        .build();

    content
        .sel(quit_id)
        .clear()
        .add_text(quit_content, SpanStyle::default())
        .build();

    // Show rich texts at specific positions
    sugarloaf.set_position(heading_id, 150.0, 270.0);
    sugarloaf.set_visibility(heading_id, true);

    sugarloaf.set_position(confirm_id, 150.0, 330.0);
    sugarloaf.set_visibility(confirm_id, true);

    sugarloaf.set_position(quit_id, 268.0, 330.0);
    sugarloaf.set_visibility(quit_id, true);
}
