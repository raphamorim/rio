use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::Sugarloaf;

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let layout = sugarloaf.window_size();

    // Render rectangles directly
    sugarloaf.rect(
        0.0,
        0.0,
        layout.width / context_dimension.dimension.scale,
        layout.height,
        [0.0, 0.0, 0.0, 1.0],
        0.0,
    );
    sugarloaf.rect(0.0, 30.0, 15.0, layout.height, [0.0, 0.0, 1.0, 1.0], 0.0);
    sugarloaf.rect(
        15.0,
        context_dimension.margin.top_y + 60.0,
        15.0,
        layout.height,
        [1.0, 1.0, 0.0, 1.0],
        0.0,
    );
    sugarloaf.rect(
        30.0,
        context_dimension.margin.top_y + 120.0,
        15.0,
        layout.height,
        [1.0, 0.0, 0.0, 1.0],
        0.0,
    );

    // let heading = sugarloaf.create_temp_rich_text();
    // let paragraph_action = sugarloaf.create_temp_rich_text();
    // let paragraph = sugarloaf.create_temp_rich_text();

    // sugarloaf.set_rich_text_font_size(&heading, 28.0);
    // sugarloaf.show_rich_text(paragraph, 70.0, context_dimension.margin.top_y + 140.0);
}
