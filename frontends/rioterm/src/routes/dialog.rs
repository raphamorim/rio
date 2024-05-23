use rio_backend::sugarloaf::components::rect::Rect;
use rio_backend::sugarloaf::Sugarloaf;

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, content: &str) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let black = [0.0, 0.0, 0.0, 1.0];

    let layout = sugarloaf.layout();
    let height = layout.height / layout.dimensions.scale;

    let assistant_background = vec![
        Rect {
            position: [0., 0.0],
            color: black,
            size: [layout.width, layout.height],
        },
        Rect {
            position: [0., 30.0],
            color: blue,
            size: [30., layout.height],
        },
        Rect {
            position: [15., layout.margin.top_y + 40.],
            color: yellow,
            size: [30., layout.height],
        },
        Rect {
            position: [30., layout.margin.top_y + 120.],
            color: red,
            size: [30., layout.height],
        },
    ];

    sugarloaf.append_rects(assistant_background);

    let mid_screen = height / 2.;

    sugarloaf.text(
        (70., mid_screen - 10.),
        content.to_string(),
        48.,
        [1., 1., 1., 1.],
        true,
    );

    sugarloaf.text(
        (70., mid_screen + 30.),
        String::from("To quit press enter key"),
        18.,
        yellow,
        true,
    );

    sugarloaf.text(
        (70., mid_screen + 50.),
        String::from("To continue press escape key"),
        18.,
        blue,
        true,
    );
}
