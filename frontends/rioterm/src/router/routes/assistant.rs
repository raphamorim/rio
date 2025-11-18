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

    // Create transient text elements (rendered once then cleaned up)
    let heading_idx = sugarloaf.text(None);
    let action_idx = sugarloaf.text(None);
    let paragraph_idx = sugarloaf.text(None);

    // Use proportional text rendering (not monospace grid)
    sugarloaf.set_transient_use_grid_cell_size(heading_idx, false);
    sugarloaf.set_transient_use_grid_cell_size(action_idx, false);
    sugarloaf.set_transient_use_grid_cell_size(paragraph_idx, false);

    sugarloaf.set_transient_text_font_size(heading_idx, 28.0);
    sugarloaf.set_transient_text_font_size(action_idx, 18.0);
    sugarloaf.set_transient_text_font_size(paragraph_idx, 14.0);

    // Add text content to transient elements
    if let Some(heading_state) = sugarloaf.get_transient_text_mut(heading_idx) {
        heading_state
            .clear()
            .add_span("Woops! Rio got errors", SpanStyle::default())
            .build();
    }

    if let Some(action_state) = sugarloaf.get_transient_text_mut(action_idx) {
        action_state
            .clear()
            .add_span(
                "> press enter to continue",
                SpanStyle {
                    color: yellow,
                    ..SpanStyle::default()
                },
            )
            .build();
    }

    if let Some(report) = &assistant.inner {
        if let Some(paragraph_state) = sugarloaf.get_transient_text_mut(paragraph_idx) {
            paragraph_state.clear();
            for line in report.report.to_string().lines() {
                paragraph_state
                    .add_span(line, SpanStyle::default())
                    .new_line();
            }
            paragraph_state.build();
        }
    }

    // Show rich texts at specific positions
    sugarloaf.set_transient_position(
        heading_idx,
        70.0,
        context_dimension.margin.top_y + 30.0,
    );
    sugarloaf.set_transient_visibility(heading_idx, true);

    sugarloaf.set_transient_position(
        action_idx,
        70.0,
        context_dimension.margin.top_y + 70.0,
    );
    sugarloaf.set_transient_visibility(action_idx, true);

    if assistant.inner.is_some() {
        sugarloaf.set_transient_position(
            paragraph_idx,
            70.0,
            context_dimension.margin.top_y + 140.0,
        );
        sugarloaf.set_transient_visibility(paragraph_idx, true);
    }
}
