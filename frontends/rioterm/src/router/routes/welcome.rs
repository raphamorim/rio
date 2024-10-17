use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{Object, Rect, Sugarloaf, Text};

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let black = [0.0, 0.0, 0.0, 1.0];

    let layout = sugarloaf.window_size();
    let width = layout.width / sugarloaf.style().scale_factor;

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
        position: [15., context_dimension.margin.top_y + 60.],
        color: yellow,
        size: [30., layout.height],
    }));
    objects.push(Object::Rect(Rect {
        position: [30., context_dimension.margin.top_y + 120.],
        color: red,
        size: [30., layout.height],
    }));

    if width <= 440. {
        objects.push(Object::Text(Text::single_line(
            (70., context_dimension.margin.top_y + 50.),
            String::from("Welcome to\nRio Terminal"),
            28.,
            [1., 1., 1., 1.],
        )));

        objects.push(Object::Text(Text::single_line(
            (70., context_dimension.margin.top_y + 100.),
            String::from("> enter to continue"),
            18.,
            yellow,
        )));

        return;
    }

    objects.push(Object::Text(Text::single_line(
        (70., context_dimension.margin.top_y + 50.),
        String::from("Welcome to Rio Terminal"),
        28.,
        [1., 1., 1., 1.],
    )));

    objects.push(Object::Text(Text::single_line(
        (70., context_dimension.margin.top_y + 80.),
        String::from("> press enter to continue"),
        18.,
        yellow,
    )));

    objects.push(Object::Text(Text::multi_line(
        (70., context_dimension.margin.top_y + 220.),
        welcome_content(),
        18.,
        [1., 1., 1., 1.],
    )));

    sugarloaf.set_objects(objects);
}

#[inline]
fn welcome_content() -> String {
    #[cfg(target_os = "macos")]
    let shortcut = "\"Command\" + \",\" (comma)";

    #[cfg(not(target_os = "macos"))]
    let shortcut = "\"Control\" + \"Shift\" + \",\" (comma)";

    format!("Your configuration file will be created in\n{}\n\nTo open settings menu use\n{}\n\n\n\nMore info in raphamorim.io/rio
    ", rio_backend::config::config_file_path().display(), shortcut)
}
