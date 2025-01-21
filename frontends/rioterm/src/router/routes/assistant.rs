use crate::context::grid::ContextDimension;
use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::{Object, Rect, Sugarloaf, Text};

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
pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    assistant: &Assistant,
) {
    let blue = [0.1764706, 0.6039216, 1.0, 1.0];
    let yellow = [0.9882353, 0.7294118, 0.15686275, 1.0];
    let red = [1.0, 0.07058824, 0.38039216, 1.0];
    let layout = sugarloaf.window_size();

    let mut objects = Vec::with_capacity(8);

    objects.push(Object::Rect(Rect {
        position: [0., 30.0],
        color: blue,
        size: [30., layout.height],
    }));
    objects.push(Object::Rect(Rect {
        position: [15., context_dimension.margin.top_y + 40.],
        color: yellow,
        size: [30., layout.height],
    }));
    objects.push(Object::Rect(Rect {
        position: [30., context_dimension.margin.top_y + 120.],
        color: red,
        size: [30., layout.height],
    }));

    objects.push(Object::Text(Text::single_line(
        (70., context_dimension.margin.top_y + 50.),
        String::from("Woops! Rio got errors"),
        28.,
        [1., 1., 1., 1.],
    )));

    if let Some(report) = &assistant.inner {
        if report.level == RioErrorLevel::Error {
            objects.push(Object::Text(Text::single_line(
                (70., context_dimension.margin.top_y + 80.),
                String::from("after fix it, restart the terminal"),
                18.,
                [1., 1., 1., 1.],
            )));
        }

        if report.level == RioErrorLevel::Warning {
            objects.push(Object::Text(Text::single_line(
                (70., context_dimension.margin.top_y + 80.),
                String::from("(press enter to continue)"),
                18.,
                [1., 1., 1., 1.],
            )));
        }

        objects.push(Object::Text(Text::multi_line(
            (70., context_dimension.margin.top_y + 170.),
            report.report.to_string(),
            14.,
            [1., 1., 1., 1.],
        )));

        sugarloaf.set_objects(objects);
    }
}
