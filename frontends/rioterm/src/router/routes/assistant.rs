use crate::context::grid::ContextDimension;
use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

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
        None,
        0.0,
        0.0,
        layout.width / context_dimension.dimension.scale,
        layout.height,
        black,
        0.0,
    );
    sugarloaf.rect(None, 0.0, 30.0, 15.0, layout.height, blue, 0.0);
    sugarloaf.rect(
        None,
        15.0,
        context_dimension.margin.top_y + 60.0,
        15.0,
        layout.height,
        yellow,
        0.0,
    );
    sugarloaf.rect(
        None,
        30.0,
        context_dimension.margin.top_y + 120.0,
        15.0,
        layout.height,
        red,
        0.0,
    );

    // Use simple IDs for transient UI elements (not cached)
    let heading_id = 1000;
    let action_id = 1001;
    let paragraph_id = 1002;

    let _ = sugarloaf.text(heading_id);
    let _ = sugarloaf.text(action_id);
    let _ = sugarloaf.text(paragraph_id);

    sugarloaf.set_text_font_size(&heading_id, 28.0);
    sugarloaf.set_text_font_size(&action_id, 18.0);
    sugarloaf.set_text_font_size(&paragraph_id, 14.0);

    let content = sugarloaf.content();
    let heading_line = content.sel(heading_id);
    heading_line
        .clear()
        .add_text("Woops! Rio got errors", SpanStyle::default())
        .build();

    let paragraph_action_line = content.sel(action_id);
    paragraph_action_line
        .clear()
        .add_text(
            "> press enter to continue",
            SpanStyle {
                color: yellow,
                ..SpanStyle::default()
            },
        )
        .build();

    if let Some(report) = &assistant.inner {
        let paragraph_line = content.sel(paragraph_id).clear();

        for line in report.report.to_string().lines() {
            paragraph_line.add_text(line, SpanStyle::default());
        }

        paragraph_line.build();
    }

    // Show rich texts at specific positions
    sugarloaf.set_position(heading_id, 70.0, context_dimension.margin.top_y + 30.0);
    sugarloaf.set_visibility(heading_id, true);

    sugarloaf.set_position(action_id, 70.0, context_dimension.margin.top_y + 70.0);
    sugarloaf.set_visibility(action_id, true);

    if assistant.inner.is_some() {
        sugarloaf.set_position(
            paragraph_id,
            70.0,
            context_dimension.margin.top_y + 140.0,
        );
        sugarloaf.set_visibility(paragraph_id, true);
    }
}
