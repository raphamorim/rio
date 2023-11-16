use rio_backend::sugarloaf::components::rect::Rect;
use rio_backend::sugarloaf::Sugarloaf;

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, content: &str) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];

    let height = sugarloaf.layout.height / sugarloaf.layout.scale_factor;

    let assistant_background = vec![
        Rect {
            position: [0., 30.0],
            color: blue,
            size: [30., sugarloaf.layout.height],
        },
        Rect {
            position: [15., sugarloaf.layout.margin.top_y + 40.],
            color: yellow,
            size: [30., sugarloaf.layout.height],
        },
        Rect {
            position: [30., sugarloaf.layout.margin.top_y + 120.],
            color: red,
            size: [30., sugarloaf.layout.height],
        },
    ];

    sugarloaf.pile_rects(assistant_background);

    let mid_screen = height / 2.;

    sugarloaf.text(
        (70., mid_screen - 10.),
        content.to_string(),
        8,
        48.,
        [1., 1., 1., 1.],
        true,
    );

    sugarloaf.text(
        (70., mid_screen + 30.),
        String::from("To continue press enter key"),
        8,
        18.,
        yellow,
        true,
    );

    sugarloaf.text(
        (70., mid_screen + 50.),
        String::from("To quit press escape key"),
        8,
        18.,
        blue,
        true,
    );
}
