use crate::constants::*;
use rio_backend::config::colors::Colors;
use rio_backend::sugarloaf::{Object, Quad, RichText};

#[inline]
pub fn draw_search_bar(
    objects: &mut Vec<Object>,
    rich_text_id: usize,
    colors: &Colors,
    dimensions: (f32, f32, f32),
) {
    let (width, height, scale) = dimensions;
    let position_y = (height / scale) - PADDING_Y_BOTTOM_TABS;

    objects.push(Object::Quad(Quad {
        position: [0.0, position_y],
        color: colors.bar,
        size: [width, PADDING_Y_BOTTOM_TABS],
        ..Quad::default()
    }));

    objects.push(Object::RichText(RichText {
        id: rich_text_id,
        position: [4., position_y],
        lines: None,
    }));
}
