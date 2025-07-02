use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Object, Quad, RichText, Sugarloaf};

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let black = [0.0, 0.0, 0.0, 1.0];

    let layout = sugarloaf.window_size();

    let mut objects = Vec::with_capacity(7);

    objects.push(Object::Quad(Quad {
        position: [0., 0.0],
        color: black,
        size: [
            layout.width / context_dimension.dimension.scale,
            layout.height,
        ],
        ..Quad::default()
    }));
    objects.push(Object::Quad(Quad {
        position: [0., 30.0],
        color: blue,
        size: [15., layout.height],
        ..Quad::default()
    }));
    objects.push(Object::Quad(Quad {
        position: [15., context_dimension.margin.top_y + 60.],
        color: yellow,
        size: [15., layout.height],
        ..Quad::default()
    }));
    objects.push(Object::Quad(Quad {
        position: [30., context_dimension.margin.top_y + 120.],
        color: red,
        size: [15., layout.height],
        ..Quad::default()
    }));

    let heading = sugarloaf.create_temp_rich_text();
    let paragraph_action = sugarloaf.create_temp_rich_text();
    let paragraph = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&heading, 28.0);
    sugarloaf.set_rich_text_font_size(&paragraph_action, 18.0);
    sugarloaf.set_rich_text_font_size(&paragraph, 16.0);

    let content = sugarloaf.content();
    let heading_line = content.sel(heading);
    heading_line
        .clear()
        .add_text("Welcome to Rio Terminal", FragmentStyle::default())
        .build();

    let paragraph_action_line = content.sel(paragraph_action);
    paragraph_action_line
        .clear()
        .add_text(
            "> press enter to continue",
            FragmentStyle {
                color: yellow,
                ..FragmentStyle::default()
            },
        )
        .build();

    #[cfg(target_os = "macos")]
    let shortcut = "\"Command\" + \",\" (comma)";

    #[cfg(not(target_os = "macos"))]
    let shortcut = "\"Control\" + \"Shift\" + \",\" (comma)";

    let paragraph_line = content.sel(paragraph);
    paragraph_line
        .clear()
        .add_text(
            "Your configuration file will be created in",
            FragmentStyle::default(),
        )
        .new_line()
        .add_text(
            &format!(" {} ", rio_backend::config::config_file_path().display()),
            FragmentStyle {
                background_color: Some(yellow),
                color: [0., 0., 0., 1.],
                ..FragmentStyle::default()
            },
        )
        .new_line()
        .add_text("", FragmentStyle::default())
        .new_line()
        .add_text("To open settings menu use", FragmentStyle::default())
        .new_line()
        .add_text(
            &format!(" {shortcut} "),
            FragmentStyle {
                background_color: Some(yellow),
                color: [0., 0., 0., 1.],
                ..FragmentStyle::default()
            },
        )
        .new_line()
        .add_text("", FragmentStyle::default())
        .new_line()
        .add_text("", FragmentStyle::default())
        .new_line()
        .add_text("More info in rioterm.com", FragmentStyle::default())
        .build();

    objects.push(Object::RichText(RichText {
        id: heading,
        position: [70., context_dimension.margin.top_y + 30.],
        lines: None,
    }));

    objects.push(Object::RichText(RichText {
        id: paragraph_action,
        position: [70., context_dimension.margin.top_y + 70.],
        lines: None,
    }));

    objects.push(Object::RichText(RichText {
        id: paragraph,
        position: [70., context_dimension.margin.top_y + 140.],
        lines: None,
    }));

    sugarloaf.set_objects(objects);
}
