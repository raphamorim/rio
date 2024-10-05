use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{Object, Rect, Sugarloaf, Text};

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    content: &str,
) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let black = [0.0, 0.0, 0.0, 1.0];

    let layout = sugarloaf.window_size();
    let height = layout.height / sugarloaf.style().scale_factor;

    let mut objects = Vec::with_capacity(7);

    objects.push(Object::Rect(Rect {
        position: [0., 0.0],
        color: black,
        size: [layout.width, layout.height],
    }));
    objects.push(Object::Rect(Rect {
        position: [0., 30.0],
        color: blue,
        size: [30., layout.height],
    }));
    objects.push(Object::Rect(Rect {
        position: [15., context_dimension.margin.top_y + 40.],
        color: yellow,
        size: [30., layout.height],
    }));
    objects.push(Object::Rect(Rect {
        position: [30., context_dimension.margin.top_y + 120.],
        color: red,
        size: [30., layout.height],
    }));

    let mid_screen = height / 2.;

    objects.push(Object::Text(Text::single_line(
        (70., mid_screen - 10.),
        content.to_string(),
        48.,
        [1., 1., 1., 1.],
    )));

    objects.push(Object::Text(Text::single_line(
        (70., mid_screen + 30.),
        String::from("To quit press enter key"),
        18.,
        yellow,
    )));

    objects.push(Object::Text(Text::single_line(
        (70., mid_screen + 50.),
        String::from("To continue press escape key"),
        18.,
        blue,
    )));

    sugarloaf.set_objects(objects);
}
