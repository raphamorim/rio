use rio_backend::sugarloaf::{Object, Rect, Text};

#[inline]
pub fn draw_search_bar(
    objects: &mut Vec<Object>,
    dimensions: (f32, f32),
    scale: f32,
    content: &String,
) {
    let (width, height) = dimensions;
    let position_y = (height / scale) - 40.;

    objects.push(Object::Rect(Rect {
        position: [0.0, position_y],
        color: [0.4, 0.8, 1.0, 1.0],
        size: [width * (scale + 1.0), 22.0],
    }));

    objects.push(Object::Text(Text::single_line(
        (4., position_y + 10.),
        format!("Search: {}", content),
        14.,
        [1., 1., 1., 1.],
    )));
}
