use rio_config::colors::Colors;
use sugarloaf::components::rect::Rect;
use sugarloaf::font::{FONT_ID_BUILTIN, FONT_ID_ICONS};
use sugarloaf::Sugarloaf;

#[inline]
pub fn render(
    sugarloaf: &mut Sugarloaf,
    named_colors: &Colors,
    settings: &crate::router::settings::Settings,
) {
    let settings_background = vec![
        Rect {
            position: [0., 100.0],
            color: named_colors.dim_black,
            size: [sugarloaf.layout.width * 2., sugarloaf.layout.height],
        },
        Rect {
            position: [0., 96.0],
            color: named_colors.cyan,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., 104.0],
            color: named_colors.yellow,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., 112.0],
            color: named_colors.red,
            size: [sugarloaf.layout.width * 2., 8.],
        },
        Rect {
            position: [0., 180.0],
            color: [1., 1., 1., 1.],
            size: [sugarloaf.layout.width * 2., 50.],
        },
    ];

    sugarloaf.pile_rects(settings_background);

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 30.),
        "Settings".to_string(),
        FONT_ID_BUILTIN,
        28.,
        named_colors.cyan,
        true,
    );

    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 60.),
        format!(
            "{} • v{}",
            settings.default_file_path,
            env!("CARGO_PKG_VERSION")
        ),
        FONT_ID_BUILTIN,
        15.,
        named_colors.cyan,
        false,
    );

    let items_len = settings.inner.len();
    sugarloaf.text(
        (10., sugarloaf.layout.margin.top_y + 130.),
        String::from(""),
        FONT_ID_ICONS,
        16.,
        named_colors.cursor,
        true,
    );

    let previous_item = if settings.state.current > 0 {
        settings.state.current - 1
    } else {
        items_len - 1
    };

    if let Some(prev_setting) = settings.inner.get(&previous_item) {
        sugarloaf.text(
            (10., sugarloaf.layout.margin.top_y + 150.),
            format!(
                "{} | \"{}\"",
                prev_setting.title, prev_setting.options[prev_setting.current_option],
            ),
            FONT_ID_BUILTIN,
            16.,
            named_colors.dim_white,
            true,
        );
    }

    if let Some(active_setting) = settings.inner.get(&settings.state.current) {
        sugarloaf.text(
            (60., sugarloaf.layout.margin.top_y + 190.),
            format!(
                "{} | {:?}",
                active_setting.title,
                active_setting.options[active_setting.current_option],
            ),
            FONT_ID_BUILTIN,
            18.,
            named_colors.background.0,
            true,
        );

        if active_setting.requires_restart {
            sugarloaf.text(
                (
                    sugarloaf.layout.width / sugarloaf.layout.scale_factor - 160.,
                    sugarloaf.layout.margin.top_y + 225.,
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
                sugarloaf.layout.margin.top_y + 190.,
            ),
            "󰁔".to_string(),
            FONT_ID_ICONS,
            28.,
            named_colors.background.0,
            true,
        );

        sugarloaf.text(
            (10., sugarloaf.layout.margin.top_y + 190.),
            "󰁍".to_string(),
            FONT_ID_ICONS,
            28.,
            named_colors.background.0,
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
    let mut spacing_between = 230.;
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
                named_colors.dim_white,
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
        named_colors.cursor,
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
