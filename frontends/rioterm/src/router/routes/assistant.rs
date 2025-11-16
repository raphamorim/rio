use crate::context::grid::ContextDimension;
use rio_backend::error::{RioError, RioErrorLevel};
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};

// Rich text ID constants for assistant screen
const ASSISTANT_HEADING_ID: usize = 300_000;
const ASSISTANT_PARAGRAPH_ACTION_ID: usize = 300_001;
const ASSISTANT_PARAGRAPH_ID: usize = 300_002;

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

    let _ = sugarloaf.text(ASSISTANT_HEADING_ID);
    let _ = sugarloaf.text(ASSISTANT_PARAGRAPH_ACTION_ID);
    let _ = sugarloaf.text(ASSISTANT_PARAGRAPH_ID);

    sugarloaf.set_text_font_size(&ASSISTANT_HEADING_ID, 28.0);
    sugarloaf.set_text_font_size(&ASSISTANT_PARAGRAPH_ACTION_ID, 18.0);
    sugarloaf.set_text_font_size(&ASSISTANT_PARAGRAPH_ID, 14.0);

    let content = sugarloaf.content();
    let heading_line = content.sel(ASSISTANT_HEADING_ID);
    heading_line
        .clear()
        .add_text("Woops! Rio got errors", SpanStyle::default())
        .build();

    let paragraph_action_line = content.sel(ASSISTANT_PARAGRAPH_ACTION_ID);
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
        let paragraph_line = content.sel(ASSISTANT_PARAGRAPH_ID).clear();

        for line in report.report.to_string().lines() {
            paragraph_line.add_text(line, SpanStyle::default());
        }

        paragraph_line.build();
    }

    // Show rich texts at specific positions
    sugarloaf.set_position(
        ASSISTANT_HEADING_ID,
        70.0,
        context_dimension.margin.top_y + 30.0,
    );
    sugarloaf.set_visibility(ASSISTANT_HEADING_ID, true);

    sugarloaf.set_position(
        ASSISTANT_PARAGRAPH_ACTION_ID,
        70.0,
        context_dimension.margin.top_y + 70.0,
    );
    sugarloaf.set_visibility(ASSISTANT_PARAGRAPH_ACTION_ID, true);

    if assistant.inner.is_some() {
        sugarloaf.set_position(
            ASSISTANT_PARAGRAPH_ID,
            70.0,
            context_dimension.margin.top_y + 140.0,
        );
        sugarloaf.set_visibility(ASSISTANT_PARAGRAPH_ID, true);
    }
}
