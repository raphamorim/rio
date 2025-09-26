use crate::constants::*;
use rio_backend::config::colors::Colors;
use rio_backend::sugarloaf::{Object, Rect, RichText};

#[inline]
pub fn draw_search_bar(
    objects: &mut Vec<Object>,
    rich_text_id: usize,
    colors: &Colors,
    dimensions: (f32, f32, f32),
) {
    let (width, height, scale) = dimensions;
    let position_y = (height / scale) - PADDING_Y_BOTTOM_TABS;

    objects.push(Object::Rect(Rect::new(0.0, position_y, width, PADDING_Y_BOTTOM_TABS, colors.bar)));

    objects.push(Object::RichText(RichText {
        id: rich_text_id,
        lines: None,
        render_data: rio_backend::sugarloaf::RichTextRenderData {
            position: [4., position_y],
            should_repaint: false,
            should_remove: false,
            hidden: false,
        },
    }));
}
