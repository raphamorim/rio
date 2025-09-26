use crate::context::grid::ContextDimension;
use rio_backend::sugarloaf::{FragmentStyle, Sugarloaf};

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let layout = sugarloaf.window_size();

    // Render rectangles directly
    sugarloaf.add_rect(0.0, 0.0, layout.width / context_dimension.dimension.scale, layout.height, [0.0, 0.0, 0.0, 1.0]);
    sugarloaf.add_rect(0.0, 30.0, 15.0, layout.height, [0.0, 0.0, 1.0, 1.0]);
    sugarloaf.add_rect(15.0, context_dimension.margin.top_y + 60.0, 15.0, layout.height, [1.0, 1.0, 0.0, 1.0]);
    sugarloaf.add_rect(30.0, context_dimension.margin.top_y + 120.0, 15.0, layout.height, [1.0, 0.0, 0.0, 1.0]);

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
        .add_text(
            "Welcome to Rio",
            FragmentStyle {
                color: [0.9019608, 0.494118, 0.13333334, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            " terminal",
            FragmentStyle {
                color: [1.0, 1.0, 1.0, 1.0],
                ..FragmentStyle::default()
            },
        )
        .build();

    let paragraph_action_line = content.sel(paragraph_action);
    paragraph_action_line
        .clear()
        .add_text(
            "> Hint: Use ",
            FragmentStyle {
                color: [0.7019608, 0.7019608, 0.7019608, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            "config edit",
            FragmentStyle {
                color: [0.7019608, 0.7019608, 0.7019608, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            " to open configuration file",
            FragmentStyle {
                color: [0.7019608, 0.7019608, 0.7019608, 1.0],
                ..FragmentStyle::default()
            },
        )
        .build();

    let paragraph_line = content.sel(paragraph).clear();
    paragraph_line
        .add_text(
            "\n\nBuilt in Rust with:",
            FragmentStyle {
                color: [1.0, 1.0, 1.0, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            "\n• WGPU as rendering backend",
            FragmentStyle {
                color: [1.0, 1.0, 1.0, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            "\n• Tokio as async runtime",
            FragmentStyle {
                color: [1.0, 1.0, 1.0, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            "\n• Sugarloaf for Advanced Text Rendering",
            FragmentStyle {
                color: [1.0, 1.0, 1.0, 1.0],
                ..FragmentStyle::default()
            },
        )
        .add_text(
            "\n• And lots of ❤️",
            FragmentStyle {
                color: [1.0, 1.0, 1.0, 1.0],
                ..FragmentStyle::default()
            },
        )
        .build();

    // Show rich texts at specific positions
    sugarloaf.show_rich_text(heading, 70.0, context_dimension.margin.top_y + 30.0);
    sugarloaf.show_rich_text(paragraph_action, 70.0, context_dimension.margin.top_y + 70.0);
    sugarloaf.show_rich_text(paragraph, 70.0, context_dimension.margin.top_y + 140.0);
}