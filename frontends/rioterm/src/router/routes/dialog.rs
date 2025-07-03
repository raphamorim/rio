use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

#[inline]
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    heading_content: &str,
    confirm_content: &str,
    quit_content: &str,
) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let black = [0.0, 0.0, 0.0, 1.0];

    let layout = sugarloaf.window_size();

    let mut objects = Vec::with_capacity(7);

    objects.push(Object::Quad(Quad::solid(
        [0., 0.0],
        [layout.width, layout.height],
        black,
    )));
    objects.push(Object::Quad(Quad::solid(
        [0., 30.0],
        [30., layout.height],
        blue,
    )));
    objects.push(Object::Quad(Quad::solid(
        [15., context_dimension.margin.top_y + 60.],
        [30., layout.height],
        yellow,
    )));
    objects.push(Object::Quad(Quad::solid(
        [30., context_dimension.margin.top_y + 120.],
        [30., layout.height],
        red,
    )));

    let heading = sugarloaf.create_temp_rich_text();
    let confirm = sugarloaf.create_temp_rich_text();
    let quit = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&heading, 28.0);
    sugarloaf.set_rich_text_font_size(&confirm, 18.0);
    sugarloaf.set_rich_text_font_size(&quit, 18.0);

    let content = sugarloaf.content();

    let heading_line = content.sel(heading).clear();
    for line in heading_content.to_string().lines() {
        heading_line.add_text(line, FragmentStyle::default());
    }
    heading_line.build();

    objects.push(Object::RichText(RichText {
        id: heading,
        position: [70., context_dimension.margin.top_y + 30.],
        lines: None,
    }));

    let confirm_line = content.sel(confirm);
    confirm_line
        .clear()
        .add_text(
            &format!(" {confirm_content} "),
            FragmentStyle {
                color: [0., 0., 0., 1.],
                background_color: Some(yellow),
                ..FragmentStyle::default()
            },
        )
        .build();

    objects.push(Object::RichText(RichText {
        id: confirm,
        position: [70., context_dimension.margin.top_y + 100.],
        lines: None,
    }));

    let quit_line = content.sel(quit);
    quit_line
        .clear()
        .add_text(
            &format!(" {quit_content} "),
            FragmentStyle {
                color: [0., 0., 0., 1.],
                background_color: Some(red),
                ..FragmentStyle::default()
            },
        )
        .build();

    objects.push(Object::RichText(RichText {
        id: quit,
        position: [70., context_dimension.margin.top_y + 140.],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}
