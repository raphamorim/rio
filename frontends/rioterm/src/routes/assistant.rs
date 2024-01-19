use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::components::rect::Rect;
use rio_backend::sugarloaf::font::FONT_ID_BUILTIN;
use rio_backend::sugarloaf::Sugarloaf;

pub struct Assistant {
    pub inner: Option<RioError>,
}

impl Assistant {
    pub fn new() -> Assistant {
        Assistant { inner: None }
    }

    #[inline]
    pub fn set(&mut self, report: RioError) {
        self.inner = Some(report);
    }

    #[inline]
    pub fn clear(&mut self) {
        self.inner = None;
    }

    #[inline]
    #[allow(unused)]
    pub fn is_warning(&self) -> bool {
        if let Some(report) = &self.inner {
            if report.level == RioErrorLevel::Error {
                return false;
            }
        }

        true
    }
}

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, assistant: &Assistant) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];

    let assistant_background = vec![
        // Rect {
        //     position: [30., 0.0],
        //     color: self.named_colors.background.0,
        //     size: [sugarloaf.layout.width, sugarloaf.layout.height],
        // },
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

    sugarloaf.text(
        (70., sugarloaf.layout.margin.top_y + 50.),
        String::from("Woops! Rio got errors"),
        FONT_ID_BUILTIN,
        28.,
        [1., 1., 1., 1.],
        true,
    );

    if let Some(report) = &assistant.inner {
        if report.level == RioErrorLevel::Error {
            sugarloaf.text(
                (70., sugarloaf.layout.margin.top_y + 80.),
                String::from("after fix it, restart the terminal"),
                FONT_ID_BUILTIN,
                18.,
                [1., 1., 1., 1.],
                true,
            );
        }

        if report.level == RioErrorLevel::Warning {
            sugarloaf.text(
                (70., sugarloaf.layout.margin.top_y + 80.),
                String::from("(press enter to continue)"),
                FONT_ID_BUILTIN,
                18.,
                [1., 1., 1., 1.],
                true,
            );
        }

        sugarloaf.text(
            (70., sugarloaf.layout.margin.top_y + 170.),
            report.report.to_string(),
            FONT_ID_BUILTIN,
            14.,
            [1., 1., 1., 1.],
            false,
        );
    }
}
