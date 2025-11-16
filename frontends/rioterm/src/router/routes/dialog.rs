use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

// Rich text ID constants for dialog screen
const DIALOG_HEADING_ID: usize = 400_000;
const DIALOG_CONFIRM_ID: usize = 400_001;
const DIALOG_QUIT_ID: usize = 400_002;

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

    let _ = sugarloaf.text(DIALOG_HEADING_ID);
    let _ = sugarloaf.text(DIALOG_CONFIRM_ID);
    let _ = sugarloaf.text(DIALOG_QUIT_ID);

    sugarloaf.set_text_font_size(&DIALOG_HEADING_ID, 32.0);
    sugarloaf.set_text_font_size(&DIALOG_CONFIRM_ID, 20.0);
    sugarloaf.set_text_font_size(&DIALOG_QUIT_ID, 20.0);

    let content = sugarloaf.content();

    content
        .sel(DIALOG_HEADING_ID)
        .clear()
        .add_text(heading_content, SpanStyle::default())
        .build();

    content
        .sel(DIALOG_CONFIRM_ID)
        .clear()
        .add_text(confirm_content, SpanStyle::default())
        .build();

    content
        .sel(DIALOG_QUIT_ID)
        .clear()
        .add_text(quit_content, SpanStyle::default())
        .build();

    // Show rich texts at specific positions
    sugarloaf.set_position(DIALOG_HEADING_ID, 150.0, 270.0);
    sugarloaf.set_visibility(DIALOG_HEADING_ID, true);

    sugarloaf.set_position(DIALOG_CONFIRM_ID, 150.0, 330.0);
    sugarloaf.set_visibility(DIALOG_CONFIRM_ID, true);

    sugarloaf.set_position(DIALOG_QUIT_ID, 268.0, 330.0);
    sugarloaf.set_visibility(DIALOG_QUIT_ID, true);
}
