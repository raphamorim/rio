use crate::layout::ContextDimension;
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

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
    let box_x = (win_w - box_w) / 2.0;
    let box_y = (win_h - box_h) / 2.0;

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

    let heading_opts = DrawOpts {
        font_size: 13.0,
        color: [255, 255, 255, 255],
        ..DrawOpts::default()
    };
    let gray_opts = DrawOpts {
        font_size: 13.0,
        color: [166, 166, 166, 255],
        ..DrawOpts::default()
    };

    let text_x = box_x + padding_x;
    let text_y = box_y + padding_y + 2.0;

    let ui = sugarloaf.text_mut();
    let heading_w = ui.draw(text_x, text_y, heading_content, &heading_opts);
    ui.draw(
        text_x + heading_w,
        text_y,
        &format!("  {}  /  {}", confirm_content, quit_content),
        &gray_opts,
    );
}
