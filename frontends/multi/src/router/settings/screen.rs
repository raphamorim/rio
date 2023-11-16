use rio_backend::sugarloaf::components::rect::Rect;
use rio_backend::sugarloaf::font::{FONT_ID_BUILTIN, FONT_ID_ICONS};
use rio_backend::sugarloaf::Sugarloaf;

#[inline]
pub fn render(sugarloaf: &mut Sugarloaf, settings: &crate::router::settings::Settings) {
    // TODO: Refactor color management per screen
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let dim_black = [0.10980392, 0.09803922, 0.101960786, 1.0];
    let background = [0., 0., 0., 1.];
    let cursor = [0.96862745, 0.07058824, 1.0, 1.0];

    let settings_background = vec![
        Rect {
            position: [0., sugarloaf.layout.margin.top_y + 100.0],
            color: dim_black,
            size: [sugarloaf.layout.width * 2., sugarloaf.layout.height],
        },
        Rect {
            position: [0., sugarloaf.layout.margin.top_y + 96.0],
            color: blue,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., sugarloaf.layout.margin.top_y + 104.0],
            color: yellow,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., sugarloaf.layout.margin.top_y + 112.0],
            color: red,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., sugarloaf.layout.margin.top_y + 175.0],
            color: [1., 1., 1., 1.],
            size: [sugarloaf.layout.width * 2., 50.],
        },
    ];

    sugarloaf.pile_rects(settings_background);

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 50.),
        "Settings".to_string(),
        FONT_ID_BUILTIN,
        28.,
        blue,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 80.),
        format!(
            "{} • v{}",
            settings.default_file_path.display(),
            env!("CARGO_PKG_VERSION")
        ),
        FONT_ID_BUILTIN,
        15.,
        blue,
        false,
    );

    let items_len = settings.inner.len();
    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 140.),
        String::from(""),
        FONT_ID_ICONS,
        16.,
        cursor,
        true,
    );

    let previous_item = if settings.state.current > 0 {
        settings.state.current - 1
    } else {
        items_len - 1
    };

    if let Some(prev_setting) = settings.inner.get(&previous_item) {
        sugarloaf.text(
            (10., sugarloaf.layout.margin.top_y + 160.),
            format!(
                "{} | \"{}\"",
                prev_setting.title, prev_setting.options[prev_setting.current_option],
            ),
            FONT_ID_BUILTIN,
            16.,
            [1., 1., 1., 1.],
            true,
        );
    }

    if let Some(active_setting) = settings.inner.get(&settings.state.current) {
        sugarloaf.text(
            (60., sugarloaf.layout.margin.top_y + 200.),
            format!(
                "{} | {:?}",
                active_setting.title,
                active_setting.options[active_setting.current_option],
            ),
            FONT_ID_BUILTIN,
            18.,
            background,
            true,
        );

        if active_setting.requires_restart {
            sugarloaf.text(
                (
                    sugarloaf.layout.width / sugarloaf.layout.scale_factor - 160.,
                    sugarloaf.layout.margin.top_y + 235.,
                ),
                "* restart is needed".to_string(),
                FONT_ID_BUILTIN,
                14.,
                [1., 1., 1., 1.],
                true,
            );
        }

        sugarloaf.text(
            (
                sugarloaf.layout.width / sugarloaf.layout.scale_factor - 40.,
                sugarloaf.layout.margin.top_y + 200.,
            ),
            "󰁔".to_string(),
            FONT_ID_ICONS,
            28.,
            background,
            true,
        );

        sugarloaf.text(
            (10., sugarloaf.layout.margin.top_y + 200.),
            "󰁍".to_string(),
            FONT_ID_ICONS,
            28.,
            background,
            true,
        );
    }

    let mut iter = if settings.state.current + 5 >= items_len {
        Vec::from_iter(settings.state.current..items_len)
    } else {
        Vec::from_iter(settings.state.current..settings.state.current + 5)
    };

    let created_iter_len = iter.len();
    // Is always expected 5 items
    if created_iter_len < 5 {
        let diff = 5 - created_iter_len;
        for i in 0..diff {
            iter.push(i);
        }
    }

    let settings_iterator = Vec::from_iter(iter);
    let mut spacing_between = 240.;
    for i in settings_iterator {
        if i == settings.state.current {
            continue;
        }

        if let Some(setting) = settings.inner.get(&i) {
            sugarloaf.text(
                (10., sugarloaf.layout.margin.top_y + spacing_between),
                format!(
                    "{} | \"{}\"",
                    setting.title, setting.options[setting.current_option],
                ),
                FONT_ID_BUILTIN,
                16.,
                [1., 1., 1., 1.],
                true,
            );

            spacing_between += 20.;
        }
    }

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + spacing_between),
        String::from(""),
        FONT_ID_ICONS,
        16.,
        cursor,
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 30.,
        ),
        "󰌑".to_string(),
        FONT_ID_ICONS,
        26.,
        [1., 1., 1., 1.],
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 50.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 50.,
        ),
        "save".to_string(),
        FONT_ID_BUILTIN,
        14.,
        [1., 1., 1., 1.],
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 100.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 30.,
        ),
        "󱊷".to_string(),
        FONT_ID_ICONS,
        26.,
        [1., 1., 1., 1.],
        true,
    );

    sugarloaf.text(
        (
            sugarloaf.layout.width / sugarloaf.layout.scale_factor - 100.,
            sugarloaf.layout.height / sugarloaf.layout.scale_factor - 50.,
        ),
        "exit".to_string(),
        FONT_ID_BUILTIN,
        14.,
        [1., 1., 1., 1.],
        true,
    );
}
