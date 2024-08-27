use crate::constants::*;
use rio_backend::config::colors::Colors;
use rio_backend::sugarloaf::{Object, Rect, Text};

#[inline]
pub fn draw_search_bar(
    objects: &mut Vec<Object>,
    colors: &Colors,
    dimensions: (f32, f32, f32),
    content: &String,
) {
    let (width, height, scale) = dimensions;
    let position_y = (height / scale) - PADDING_Y_BOTTOM_TABS;

    objects.push(Object::Rect(Rect {
        position: [0.0, position_y],
        color: colors.bar,
        size: [
            (width * 2.) * scale,
            PADDING_Y_BOTTOM_TABS,
        ],
    }));

    if content.is_empty() {
        objects.push(Object::Text(Text::single_line(
            (4., position_y + 10.),
            String::from("Search: type something..."),
            14.,
            [
                colors.foreground[0],
                colors.foreground[1],
                colors.foreground[2],
                colors.foreground[3] - 0.3,
            ],
        )));
        return;
    }

    objects.push(Object::Text(Text::single_line(
        (4., position_y + 10.),
        format!("Search: {}", content),
        14.,
        colors.foreground,
    )));
}
