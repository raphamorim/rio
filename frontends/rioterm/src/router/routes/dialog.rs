use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Object, Rect, RichText, Sugarloaf};

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

    // objects.push(Object::Text(Text::single_line(
    //     (70., mid_screen - 10.),
    //     content.to_string(),
    //     48.,
    //     [1., 1., 1., 1.],
    // )));

    // objects.push(Object::Text(Text::single_line(
    //     (70., mid_screen + 30.),
    //     String::from("To quit press enter key"),
    //     18.,
    //     yellow,
    // )));

    // objects.push(Object::Text(Text::single_line(
    //     (70., mid_screen + 50.),
    //     String::from("To continue press escape key"),
    //     18.,
    //     blue,
    // )));

    let heading = sugarloaf.create_temp_rich_text();
    let confirm = sugarloaf.create_temp_rich_text();
    let quit = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&heading, 28.0);
    sugarloaf.set_rich_text_font_size(&confirm, 18.0);
    sugarloaf.set_rich_text_font_size(&quit, 18.0);

    let content = sugarloaf.content();

    let heading_line = content.sel(heading).clear();
    for line in heading_content.to_string().lines() {
        heading_line
            .new_line()
            .add_text(line, FragmentStyle::default());
    }
    heading_line.build();

    objects.push(Object::RichText(RichText {
        id: heading,
        position: [70., context_dimension.margin.top_y + 30.],
    }));

    let confirm_line = content.sel(confirm);
    confirm_line
        .clear()
        .new_line()
        .add_text(
            &format!(" {} ", confirm_content),
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
    }));

    let quit_line = content.sel(quit);
    quit_line
        .clear()
        .new_line()
        .add_text(
            &format!(" {} ", quit_content),
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
    }));

    sugarloaf.set_objects(objects);
}
