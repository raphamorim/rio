use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::Sugarloaf;

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let layout = sugarloaf.window_size();

    // Render rectangles directly
    sugarloaf.rect(
        None,
        0.0,
        0.0,
        layout.width / context_dimension.dimension.scale,
        layout.height,
        [0.0, 0.0, 0.0, 1.0],
        0.0,
    );
    sugarloaf.rect(None, 0.0, 30.0, 15.0, layout.height, [0.0, 0.0, 1.0, 1.0], 0.0);
    sugarloaf.rect(
        None,
        15.0,
        context_dimension.margin.top_y + 60.0,
        15.0,
        layout.height,
        [1.0, 1.0, 0.0, 1.0],
        0.0,
    );
    sugarloaf.rect(
        None,
        30.0,
        context_dimension.margin.top_y + 120.0,
        15.0,
        layout.height,
        [1.0, 0.0, 0.0, 1.0],
        0.0,
    );
}
