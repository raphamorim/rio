use crate::layout::ContextDimension;
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;
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

    let bottom_y = (layout.height / scale) - 50.0;
    let ui = sugarloaf.text_mut();
    ui.draw(
        20.0,
        bottom_y,
        "press enter",
        &DrawOpts {
            font_size: 14.0,
            color: [255, 255, 255, 255],
            ..DrawOpts::default()
        },
    );

    let path = rio_backend::config::config_file_path();
    ui.draw(
        20.0,
        (layout.height / scale) - 30.0,
        &path.display().to_string(),
        &DrawOpts {
            font_size: 14.0,
            color: [128, 128, 128, 255],
            ..DrawOpts::default()
        },
    );
}
