use colors::Colors;
use sugarloaf::components::rect::Rect;
use sugarloaf::Sugarloaf;

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, named_colors: &Colors) {
    let width = sugarloaf.layout.width / sugarloaf.layout.scale_factor;

    let assistant_background = vec![
        Rect {
            position: [0., 30.0],
            color: named_colors.blue,
            size: [30., sugarloaf.layout.height],
        },
        Rect {
            position: [15., sugarloaf.layout.margin.top_y + 40.],
            color: named_colors.yellow,
            size: [30., sugarloaf.layout.height],
        },
        Rect {
            position: [30., sugarloaf.layout.margin.top_y + 120.],
            color: named_colors.red,
            size: [30., sugarloaf.layout.height],
        },
    ];

    sugarloaf.pile_rects(assistant_background);

    if width <= 440. {
        sugarloaf.text(
            (70., sugarloaf.layout.margin.top_y + 50.),
            "Welcome to\nRio Terminal".to_string(),
            8,
            28.,
            named_colors.foreground,
            false,
        );

        sugarloaf.text(
            (70., sugarloaf.layout.margin.top_y + 100.),
            String::from("(enter to continue)"),
            8,
            18.,
            named_colors.yellow,
            false,
        );

        sugarloaf.text(
            (width - 50., sugarloaf.layout.margin.top_y + 320.),
            "󰌑".to_string(),
            7,
            26.,
            named_colors.yellow,
            true,
        );

        sugarloaf.text(
            (width - 50., sugarloaf.layout.margin.top_y + 340.),
            "nice".to_string(),
            8,
            14.,
            named_colors.yellow,
            true,
        );

        return;
    }

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 50.),
        "Welcome to Rio Terminal".to_string(),
        8,
        28.,
        named_colors.foreground,
        true,
    );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 80.),
        String::from("(press enter to continue)"),
        8,
        18.,
        named_colors.yellow,
        true,
    );

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 220.),
        welcome_content(),
        8,
        18.,
        named_colors.foreground,
        false,
    );

    sugarloaf.text(
        (width - 50., sugarloaf.layout.margin.top_y + 320.),
        "󰌑".to_string(),
        7,
        26.,
        named_colors.yellow,
        true,
    );

    sugarloaf.text(
        (width - 50., sugarloaf.layout.margin.top_y + 340.),
        "nice".to_string(),
        8,
        14.,
        named_colors.yellow,
        true,
    );
}

#[inline]
#[cfg(target_os = "macos")]
fn welcome_content() -> String {
    format!("Your configuration file will be created in\n{}\n\nTo open settings menu use\n\"Command\" + \",\" (comma)\n\n\n\nMore info in raphamorim.io/rio/docs
    ", config::config_file_path())
}

#[inline]
#[cfg(not(target_os = "macos"))]
fn welcome_content() -> String {
    format!("Your configuration file will be created in\n{}\n\nTo open settings menu use\n\"Control\" + \"Shift\" + \",\" (comma)\n\n\n\nMore info in raphamorim.io/rio/docs
    ", config::config_file_path())
}
