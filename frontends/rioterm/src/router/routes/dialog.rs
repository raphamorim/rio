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
    let layout = sugarloaf.window_size();
    let scale = context_dimension.dimension.scale;
    let win_w = layout.width / scale;
    let win_h = layout.height / scale;

    let full_text = format!(
        "{}  {}  /  {}",
        heading_content, confirm_content, quit_content
    );
    let padding_x = 12.0;
    let padding_y = 6.0;
    let text_h = 16.0;
    let box_w = full_text.len() as f32 * 7.5 + padding_x * 2.0;
    let box_h = text_h + padding_y * 2.0;
    let box_x = win_w - box_w - 16.0;
    let box_y = win_h - box_h - 16.0;

    // Tooltip background
    sugarloaf.rect(
        None,
        box_x,
        box_y,
        box_w,
        box_h,
        [0.0, 0.0, 0.0, 1.0],
        0.0,
        20,
    );

    // Transient text — recreated each frame, cleared automatically
    let text_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(text_idx, false);
    sugarloaf.set_transient_text_font_size(text_idx, 13.0);
    sugarloaf.set_transient_order(text_idx, 20);

    if let Some(state) = sugarloaf.get_transient_text_mut(text_idx) {
        let gray = [0.65, 0.65, 0.65, 1.0];
        let white = [1.0, 1.0, 1.0, 1.0];
        state
            .clear()
            .add_span(
                heading_content,
                SpanStyle {
                    color: white,
                    ..SpanStyle::default()
                },
            )
            .add_span(
                &format!("  {}  /  {}", confirm_content, quit_content),
                SpanStyle {
                    color: gray,
                    ..SpanStyle::default()
                },
            )
            .build();
    }

    sugarloaf.set_transient_position(
        text_idx,
        box_x + padding_x,
        box_y + padding_y + 2.0,
    );
    sugarloaf.set_transient_visibility(text_idx, true);
}
