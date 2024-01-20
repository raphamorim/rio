use rio_backend::sugarloaf::components::rect::Rect;
use rio_backend::sugarloaf::Sugarloaf;

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];

    let width = sugarloaf.layout.width / sugarloaf.layout.scale_factor;

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

    if width <= 440. {
        sugarloaf.text(
            (70., sugarloaf.layout.margin.top_y + 50.),
            String::from("Welcome to\nRio Terminal"),
            8,
            28.,
            [1., 1., 1., 1.],
            false,
        );

        sugarloaf.text(
            (70., sugarloaf.layout.margin.top_y + 100.),
            String::from("(enter to continue)"),
            8,
            18.,
            yellow,
            false,
        );

        sugarloaf.text(
            (width - 50., sugarloaf.layout.margin.top_y + 320.),
            String::from("󰌑"),
            7,
            26.,
            yellow,
            true,
        );

        sugarloaf.text(
            (width - 50., sugarloaf.layout.margin.top_y + 340.),
            String::from("nice"),
            8,
            14.,
            yellow,
            true,
        );

        return;
    }

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 50.),
        String::from("Welcome to Rio Terminal"),
        8,
        28.,
        [1., 1., 1., 1.],
        true,
    );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 80.),
        String::from("(press enter to continue)"),
        8,
        18.,
        yellow,
        true,
    );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 220.),
        welcome_content(),
        8,
        18.,
        [1., 1., 1., 1.],
        false,
    );

    sugarloaf.text(
        (width - 50., sugarloaf.layout.margin.top_y + 320.),
        String::from("󰌑"),
        7,
        26.,
        yellow,
        true,
    );

    sugarloaf.text(
        (width - 50., sugarloaf.layout.margin.top_y + 340.),
        String::from("nice"),
        8,
        14.,
        yellow,
        true,
    );
}

#[inline]
fn welcome_content() -> String {
    #[cfg(target_os = "macos")]
    let shortcut = "\"Command\" + \",\" (comma)";

    #[cfg(not(target_os = "macos"))]
    let shortcut = "\"Control\" + \"Shift\" + \",\" (comma)";

    format!("Your configuration file will be created in\n{}\n\nTo open settings menu use\n{}\n\n\n\nMore info in raphamorim.io/rio/docs
    ", rio_backend::config::config_file_path().display(), shortcut)
}
