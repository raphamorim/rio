use rio_backend::config::colors::Colors;
use crate::constants::*;
use rio_backend::sugarloaf::{Object, Rect, Text};

#[inline]
pub fn draw_search_bar(
    objects: &mut Vec<Object>,
    colors: &Colors,
    dimensions: (f32, f32),
    scale: f32,
    content: &String,
) {
    let (width, height) = dimensions;
    let position_y = (height / scale) - PADDING_Y_BOTTOM_TABS;

    objects.push(Object::Rect(Rect {
        position: [0.0, position_y],
        color: colors.bar,
        size: [(width + PADDING_Y_BOTTOM_TABS) * scale, PADDING_Y_BOTTOM_TABS],
    }));

    objects.push(Object::Text(Text::single_line(
        (4., position_y + 10.),
        format!("Search: {}", content),
        14.,
        colors.foreground,
    )));
}
