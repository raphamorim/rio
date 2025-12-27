use crate::layout::ContextDimension;
use rio_backend::sugarloaf::{drawable_character, SpanStyle, Sugarloaf};
use std::sync::OnceLock;
use std::time::Instant;

static START_TIME: OnceLock<Instant> = OnceLock::new();

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let start = *START_TIME.get_or_init(Instant::now);
    let elapsed_ms = start.elapsed().as_millis() as usize;
    let filled_stripes = (elapsed_ms / 150).min(7); // Fill one more gap every 150ms
    let bg_color = [0.02, 0.02, 0.05, 1.0];
    let stripe_color = [1.0, 1.0, 1.0, 1.0]; // White

    let layout = sugarloaf.window_size();
    let scale = context_dimension.dimension.scale;
    let width = layout.width / scale;
    let height = layout.height / scale;

    // Background
    sugarloaf.rect(None, 0.0, 0.0, width, height, bg_color, 0.0, 0);

    // TV border dimensions (80% of window size)
    let border_thickness = 3.0;
    let border_color = [1.0, 1.0, 1.0, 1.0]; // White border

    let border_width = width * 0.8;
    let border_height = height * 0.8;
    let border_left = (width - border_width) / 2.0;
    let border_top = (height - border_height) / 2.0;

    // Letter dimensions (1.5x size)
    let stripe_height = 9.0;
    let stripe_gap = 3.0; // Gap between stripes
    let stripe_step = stripe_height + stripe_gap;
    let num_stripes = 7;
    let letter_height = num_stripes as f32 * stripe_step - stripe_gap;
    let letter_width = 90.0;
    let letter_spacing = 30.0;

    // Position RIO left-aligned with config text
    let start_x = border_left + 20.0;
    let start_y = (height - letter_height) / 2.0 - 40.0;

    let horizontal_gap = 3.0;
    let segment_height = (stripe_height - horizontal_gap) / 2.0;
    let border_right = border_left + border_width;
    let border_bottom = border_top + border_height;

    // Curve depth - how much the sides bulge outward
    let curve_depth = 20.0;
    // Calculate arc radius from curve depth and height using circle geometry
    // For a chord of length H and sagitta (depth) D: R = (H²/8D) + D/2
    let arc_radius = (border_height.powi(2) / (8.0 * curve_depth)) + curve_depth / 2.0;
    let half_angle = (border_height / 2.0 / arc_radius).asin().to_degrees();

    // Top border (straight)
    sugarloaf.rect(None, border_left, border_top, border_width, border_thickness, border_color, 0.0, 0);

    // Bottom border (straight)
    sugarloaf.rect(None, border_left, border_bottom - border_thickness, border_width, border_thickness, border_color, 0.0, 0);

    // Left curved border (bulging left)
    let left_center_x = border_left + arc_radius - curve_depth;
    let left_center_y = border_top + border_height / 2.0;
    sugarloaf.arc(left_center_x, left_center_y, arc_radius, 180.0 - half_angle, 180.0 + half_angle, border_thickness, border_color, 0.0);

    // Right curved border (bulging right)
    let right_center_x = border_right - arc_radius + curve_depth;
    let right_center_y = border_top + border_height / 2.0;
    sugarloaf.arc(right_center_x, right_center_y, arc_radius, -half_angle, half_angle, border_thickness, border_color, 0.0);

    // Helper to draw a horizontal stripe split into 2 vertical rows, with gap fill animation
    let mut draw_stripe = |x: f32, y: f32, w: f32, row: usize| {
        if row < filled_stripes {
            // Draw solid rectangle that also fills the gap to the next stripe
            let fill_height = if row < num_stripes - 1 {
                stripe_step // Include gap to next stripe
            } else {
                stripe_height // Last row, no gap below
            };
            sugarloaf.rect(None, x, y, w, fill_height, stripe_color, 0.0, 0);
        } else {
            // Draw two segments with gap
            sugarloaf.rect(None, x, y, w, segment_height, stripe_color, 0.0, 0);
            sugarloaf.rect(None, x, y + segment_height + horizontal_gap, w, segment_height, stripe_color, 0.0, 0);
        }
    };

    // Letter R
    let r_x = start_x;
    for i in 0..num_stripes {
        let y = start_y + i as f32 * stripe_step;
        match i {
            0 => draw_stripe(r_x, y, letter_width * 0.8, i), // Top
            1 | 2 => {
                draw_stripe(r_x, y, letter_width * 0.25, i); // Left stem
                draw_stripe(r_x + letter_width * 0.7, y, letter_width * 0.3, i); // Right
            }
            3 => draw_stripe(r_x, y, letter_width * 0.8, i), // Middle
            4 | 5 | 6 => {
                draw_stripe(r_x, y, letter_width * 0.25, i); // Left stem
                // Diagonal leg
                let offset = (i - 3) as f32 * 18.0;
                draw_stripe(r_x + letter_width * 0.4 + offset, y, letter_width * 0.25, i);
            }
            _ => {}
        }
    }

    // Letter I
    let i_x = start_x + letter_width + letter_spacing;
    for i in 0..num_stripes {
        let y = start_y + i as f32 * stripe_step;
        match i {
            0 | 6 => draw_stripe(i_x, y, letter_width, i), // Top and bottom bars
            _ => draw_stripe(i_x + letter_width * 0.35, y, letter_width * 0.3, i), // Center stem
        }
    }

    // Letter O
    let o_x = start_x + letter_width + letter_spacing + letter_width + letter_spacing * 0.5;
    for i in 0..num_stripes {
        let y = start_y + i as f32 * stripe_step;
        match i {
            0 | 6 => {
                // Top and bottom - shorter, centered
                draw_stripe(o_x + letter_width * 0.15, y, letter_width * 0.7, i);
            }
            _ => {
                // Left and right sides
                draw_stripe(o_x, y, letter_width * 0.25, i);
                draw_stripe(o_x + letter_width * 0.75, y, letter_width * 0.25, i);
            }
        }
    }

    let ui_font_id = sugarloaf.ui_font_id();

    // Symbol next to RIO (𜱭𜱭) - uses DrawableChar rendering
    let symbol_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(symbol_idx, false);
    sugarloaf.set_transient_text_font_size(symbol_idx, letter_height);

    let drawable = drawable_character('\u{1CC6D}');
    if let Some(symbol_state) = sugarloaf.get_transient_text_mut(symbol_idx) {
        symbol_state
            .clear()
            .add_span(
                "\u{1CC6D}",
                SpanStyle {
                    font_id: ui_font_id,
                    color: stripe_color,
                    drawable_char: drawable,
                    ..Default::default()
                },
            )
            .add_span(
                "\u{1CC6D}",
                SpanStyle {
                    font_id: ui_font_id,
                    color: stripe_color,
                    drawable_char: drawable,
                    ..Default::default()
                },
            )
            .build();
    }

    // Position symbol after the O letter
    let rio_end_x = o_x + letter_width + 15.0;
    sugarloaf.set_transient_position(symbol_idx, rio_end_x, start_y);
    sugarloaf.set_transient_visibility(symbol_idx, true);

    // Title text in top-left of TV border (embedded in border line)
    let title_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(title_idx, false);
    sugarloaf.set_transient_text_font_size(title_idx, 18.0);

    if let Some(title_state) = sugarloaf.get_transient_text_mut(title_idx) {
        title_state
            .clear()
            .add_span(
                "Raphael Amorim's",
                SpanStyle {
                    font_id: ui_font_id,
                    color: border_color,
                    ..Default::default()
                },
            )
            .build();
    }

    // Position title text: top-left corner, centered on the border line
    let title_text_width = 200.0;
    let title_x = border_left + 10.0;
    let title_y = border_top - border_thickness / 2.0 - 9.0;
    sugarloaf.set_transient_position(title_idx, title_x, title_y);
    sugarloaf.set_transient_visibility(title_idx, true);

    // Draw a small background rect to "cut" the border behind the title
    let title_bg_padding = 6.0;
    sugarloaf.rect(
        None,
        title_x - title_bg_padding,
        border_top,
        title_text_width + title_bg_padding * 2.0,
        border_thickness,
        bg_color,
        0.0,
        0,
    );

    // Version text in bottom-right of TV border (embedded in border line)
    let version_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(version_idx, false);
    sugarloaf.set_transient_text_font_size(version_idx, 18.0);

    if let Some(version_state) = sugarloaf.get_transient_text_mut(version_idx) {
        version_state
            .clear()
            .add_span(
                "v0.3",
                SpanStyle {
                    font_id: ui_font_id,
                    color: border_color,
                    ..Default::default()
                },
            )
            .build();
    }

    // Position version text: bottom-right corner, centered on the border line
    let version_text_width = 50.0;
    let version_x = border_right - version_text_width - 10.0;
    let version_y = border_bottom - border_thickness / 2.0 - 9.0;
    sugarloaf.set_transient_position(version_idx, version_x, version_y);
    sugarloaf.set_transient_visibility(version_idx, true);

    // Draw a small background rect to "cut" the border behind the text
    let text_bg_padding = 6.0;
    sugarloaf.rect(
        None,
        version_x - text_bg_padding,
        border_bottom - border_thickness,
        version_text_width + text_bg_padding * 2.0,
        border_thickness,
        bg_color,
        0.0,
        0,
    );

    // Config path info text in bottom-left inside TV border
    // Animate like old computer prompt - show lines progressively
    let yellow = [0.9, 0.8, 0.2, 1.0];
    let config_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(config_idx, false);
    sugarloaf.set_transient_text_font_size(config_idx, 14.0);

    // Line timing (after RIO animation completes at ~1050ms)
    let line1_time = 1200; // "Your configuration file..."
    let line2_time = 1700; // config path
    let line3_time = 2200; // "Press enter to continue..."

    if let Some(config_state) = sugarloaf.get_transient_text_mut(config_idx) {
        config_state.clear();

        if elapsed_ms >= line1_time {
            config_state.add_span(
                "Your configuration file will be created in",
                SpanStyle {
                    font_id: ui_font_id,
                    color: border_color,
                    ..Default::default()
                },
            );
        }

        if elapsed_ms >= line2_time {
            config_state.new_line().add_span(
                &format!(" {} ", rio_backend::config::config_file_path().display()),
                SpanStyle {
                    font_id: ui_font_id,
                    color: [0.0, 0.0, 0.0, 1.0],
                    background_color: Some(yellow),
                    ..Default::default()
                },
            );
        }

        if elapsed_ms >= line3_time {
            config_state.new_line().new_line().add_span(
                "Press enter to continue...",
                SpanStyle {
                    font_id: ui_font_id,
                    color: border_color,
                    ..Default::default()
                },
            );
        }

        config_state.build();
    }

    let config_x = border_left + 20.0;
    let config_y = border_bottom - 80.0;
    sugarloaf.set_transient_position(config_idx, config_x, config_y);
    sugarloaf.set_transient_visibility(config_idx, elapsed_ms >= line1_time);
}
