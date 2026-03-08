use crate::layout::ContextDimension;
use rio_backend::sugarloaf::{SpanStyle, Sugarloaf};
use std::sync::Mutex;
use std::time::Instant;

static START_TIME: Mutex<Option<Instant>> = Mutex::new(None);

/// Draw a single logo shape (3/4 circle) with a configurable mouth opening.
/// `mouth_angle` is the half-angle of the mouth in degrees (0 = closed, 45 = wide open).
fn draw_logo(
    sugarloaf: &mut Sugarloaf,
    cx: f32,
    cy: f32,
    radius: f32,
    mouth_angle: f32,
    depth: f32,
    color: [f32; 4],
) {
    let segments = 60;
    let mut points = Vec::with_capacity(segments + 3);

    // Arc spans from mouth_angle to (360 - mouth_angle)
    let arc_span = 360.0 - 2.0 * mouth_angle;

    points.push((cx, cy));

    for i in 0..=segments {
        let angle = mouth_angle + (arc_span * i as f32 / segments as f32);
        let radians = angle * std::f32::consts::PI / 180.0;
        points.push((cx + radius * radians.cos(), cy + radius * radians.sin()));
    }

    points.push((cx, cy));

    sugarloaf.polygon(&points, depth, color);
}

#[inline]
pub fn screen(sugarloaf: &mut Sugarloaf, context_dimension: &ContextDimension) {
    let layout = sugarloaf.window_size();
    let scale = context_dimension.dimension.scale;

    // Black background
    sugarloaf.rect(
        None,
        0.0,
        0.0,
        layout.width / scale,
        layout.height,
        [0.0, 0.0, 0.0, 1.0],
        0.0,
        0,
    );

    let center_x = (layout.width / scale) / 2.0;
    let center_y = (layout.height / scale) / 2.0;

    // Compute mouth angle from time
    let now = Instant::now();
    let start = {
        let mut guard = START_TIME.lock().unwrap();
        *guard.get_or_insert(now)
    };
    let elapsed = now.duration_since(start).as_secs_f32();

    // Every 3 seconds: quick blink (close and reopen over ~0.3s)
    let cycle = elapsed % 3.0;
    let mouth_angle = if cycle > 2.7 {
        // Blink phase: 0.3s total
        // First half closes (2.7 -> 2.85), second half opens (2.85 -> 3.0)
        let blink_t = (cycle - 2.7) / 0.3;
        if blink_t < 0.5 {
            // Closing: 45° -> 0°
            45.0 * (1.0 - blink_t * 2.0)
        } else {
            // Opening: 0° -> 45°
            45.0 * (blink_t - 0.5) * 2.0
        }
    } else {
        45.0 // Normal open mouth
    };

    let white = [1.0, 1.0, 1.0, 1.0];
    let radius = 40.0;
    let gap = 2.0;

    // Two logos side by side
    draw_logo(
        sugarloaf,
        center_x - radius - gap,
        center_y,
        radius,
        mouth_angle,
        0.1,
        white,
    );
    draw_logo(
        sugarloaf,
        center_x + radius + gap,
        center_y,
        radius,
        mouth_angle,
        0.1,
        white,
    );

    // press enter
    let confirm_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(confirm_idx, false);
    sugarloaf.set_transient_text_font_size(confirm_idx, 14.0);

    if let Some(config_state) = sugarloaf.get_transient_text_mut(confirm_idx) {
        config_state
            .clear()
            .add_span("press enter", SpanStyle::default())
            .build();
    }

    sugarloaf.set_transient_position(confirm_idx, 20.0, (layout.height / scale) - 50.0);
    sugarloaf.set_transient_visibility(confirm_idx, true);

    // config path
    let config_idx = sugarloaf.text(None);
    sugarloaf.set_transient_use_grid_cell_size(config_idx, false);
    sugarloaf.set_transient_text_font_size(config_idx, 14.0);

    if let Some(config_state) = sugarloaf.get_transient_text_mut(config_idx) {
        let path = rio_backend::config::config_file_path();
        config_state
            .clear()
            .add_span(
                &path.display().to_string(),
                SpanStyle {
                    color: [0.5, 0.5, 0.5, 1.0],
                    ..SpanStyle::default()
                },
            )
            .build();
    }

    sugarloaf.set_transient_position(config_idx, 20.0, (layout.height / scale) - 30.0);
    sugarloaf.set_transient_visibility(config_idx, true);
}
