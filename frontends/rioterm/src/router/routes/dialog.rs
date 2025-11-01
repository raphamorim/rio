use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Sugarloaf};

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
        0.0,
        0.0,
        layout.width / context_dimension.dimension.scale,
        layout.height,
        [0.0, 0.0, 0.0, 0.5],
        0.0,
    );
    sugarloaf.rect(128.0, 256.0, 350.0, 150.0, [0.0, 0.0, 0.0, 1.0], 0.0);
    sugarloaf.rect(128.0, 320.0, 106.0, 36.0, [0.133, 0.141, 0.176, 1.0], 0.0);
    sugarloaf.rect(240.0, 320.0, 106.0, 36.0, [0.133, 0.141, 0.176, 1.0], 0.0);

    let heading = sugarloaf.create_temp_rich_text(None);
    let confirm = sugarloaf.create_temp_rich_text(None);
    let quit = sugarloaf.create_temp_rich_text(None);

    sugarloaf.set_rich_text_font_size(&heading, 32.0);
    sugarloaf.set_rich_text_font_size(&confirm, 20.0);
    sugarloaf.set_rich_text_font_size(&quit, 20.0);

    let content = sugarloaf.content();

    content
        .sel(heading)
        .clear()
        .add_text(heading_content, FragmentStyle::default())
        .build();

    content
        .sel(confirm)
        .clear()
        .add_text(confirm_content, FragmentStyle::default())
        .build();

    content
        .sel(quit)
        .clear()
        .add_text(quit_content, FragmentStyle::default())
        .build();

    // Show rich texts at specific positions
    sugarloaf.show_rich_text(heading, 150.0, 270.0);
    sugarloaf.show_rich_text(confirm, 150.0, 330.0);
    sugarloaf.show_rich_text(quit, 268.0, 330.0);
}
