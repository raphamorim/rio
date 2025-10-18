use crate::context::grid::ContextDimension;
use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::{FragmentStyle, Sugarloaf};

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
    let black = [0.0, 0.0, 0.0, 1.0];

    let layout = sugarloaf.window_size();

    // Render rectangles directly
    sugarloaf.rect(
        0.0,
        0.0,
        layout.width / context_dimension.dimension.scale,
        layout.height,
        black,
        0.0,
    );
    sugarloaf.rect(0.0, 30.0, 15.0, layout.height, blue, 0.0);
    sugarloaf.rect(
        15.0,
        context_dimension.margin.top_y + 60.0,
        15.0,
        layout.height,
        yellow,
        0.0,
    );
    sugarloaf.rect(
        30.0,
        context_dimension.margin.top_y + 120.0,
        15.0,
        layout.height,
        red,
        0.0,
    );

    let heading = sugarloaf.create_temp_rich_text();
    let paragraph_action = sugarloaf.create_temp_rich_text();
    let paragraph = sugarloaf.create_temp_rich_text();

    sugarloaf.set_rich_text_font_size(&heading, 28.0);
    sugarloaf.set_rich_text_font_size(&paragraph_action, 18.0);
    sugarloaf.set_rich_text_font_size(&paragraph, 14.0);

    let content = sugarloaf.content();
    let heading_line = content.sel(heading);
    heading_line
        .clear()
        .add_text("Woops! Rio got errors", FragmentStyle::default())
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

    if let Some(report) = &assistant.inner {
        let paragraph_line = content.sel(paragraph).clear();

        for line in report.report.to_string().lines() {
            paragraph_line.add_text(line, FragmentStyle::default());
        }

        paragraph_line.build();
    }

    // Show rich texts at specific positions
    sugarloaf.show_rich_text(heading, 70.0, context_dimension.margin.top_y + 30.0);
    sugarloaf.show_rich_text(
        paragraph_action,
        70.0,
        context_dimension.margin.top_y + 70.0,
    );
    if assistant.inner.is_some() {
        sugarloaf.show_rich_text(paragraph, 70.0, context_dimension.margin.top_y + 140.0);
    }
}
