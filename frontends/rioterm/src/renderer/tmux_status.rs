//! tmux's status line, drawn in the row set aside for it.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;
use rio_backend::tmux::status::{StatusColor, StatusRun, StatusStyle, TmuxStatus};

/// Under the overlays, which are free to cover it.
const ORDER: u8 = 4;
const DEPTH: f32 = 0.0;
const PADDING_X: f32 = 4.0;

/// Where to draw, in logical pixels.
pub struct Area {
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub font_size: f32,
}

fn color_u8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel.clamp(0.0, 1.0) * 255.0) as u8)
}

fn opts(style: &StatusStyle, color: [f32; 4], area: &Area) -> DrawOpts {
    DrawOpts {
        font_size: area.font_size,
        color: color_u8(color),
        bold: style.bold,
        italic: style.italic,
        ..DrawOpts::default()
    }
}

/// The colours of a style, `(foreground, background)`.
fn colors(
    style: &StatusStyle,
    resolve: &impl Fn(StatusColor, bool) -> [f32; 4],
) -> ([f32; 4], [f32; 4]) {
    let foreground = resolve(style.fg, true);
    let background = resolve(style.bg, false);
    if style.reverse {
        (background, foreground)
    } else {
        (foreground, background)
    }
}

fn width(
    sugarloaf: &mut Sugarloaf,
    runs: &[StatusRun],
    area: &Area,
    resolve: &impl Fn(StatusColor, bool) -> [f32; 4],
) -> f32 {
    let ui = sugarloaf.text_mut();
    runs.iter()
        .map(|run| {
            let (foreground, _) = colors(&run.style, resolve);
            ui.measure(&run.text, &opts(&run.style, foreground, area))
        })
        .sum()
}

/// Draw `runs` from `x`, cutting them off at `limit`.
fn draw_runs(
    sugarloaf: &mut Sugarloaf,
    runs: &[StatusRun],
    mut x: f32,
    limit: f32,
    area: &Area,
    resolve: &impl Fn(StatusColor, bool) -> [f32; 4],
) {
    let text_y = area.y + (area.height - area.font_size) / 2.0;
    for run in runs {
        let (foreground, background) = colors(&run.style, resolve);
        let opts = opts(&run.style, foreground, area);

        let mut text = run.text.as_str();
        let ui = sugarloaf.text_mut();
        let mut text_width = ui.measure(text, &opts);
        while !text.is_empty() && x + text_width > limit {
            let mut chars = text.chars();
            chars.next_back();
            text = chars.as_str();
            text_width = ui.measure(text, &opts);
        }
        if text.is_empty() {
            return;
        }

        sugarloaf.rect(
            None,
            x,
            area.y,
            text_width,
            area.height,
            background,
            DEPTH,
            ORDER,
        );
        sugarloaf.text_mut().draw(x, text_y, text, &opts);
        x += text_width;
    }
}

/// Draw the line, in `resolve`'s colours: it's given a colour, and whether
/// that's for a foreground. When there isn't room, the right side stays whole
/// and the others give way.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    status: &TmuxStatus,
    area: Area,
    resolve: impl Fn(StatusColor, bool) -> [f32; 4],
) {
    let (_, background) = colors(&status.bar.style, &resolve);
    sugarloaf.rect(
        None,
        0.0,
        area.y,
        area.width,
        area.height,
        background,
        DEPTH,
        ORDER,
    );

    let end = area.width - PADDING_X;
    let right_x = end - width(sugarloaf, &status.right, &area, &resolve);
    let centre_width = width(sugarloaf, &status.centre, &area, &resolve);
    let centre_x = (area.width - centre_width) / 2.0;
    let left_limit = if status.centre.is_empty() {
        right_x
    } else {
        right_x.min(centre_x)
    };

    draw_runs(
        sugarloaf,
        &status.left,
        PADDING_X,
        left_limit,
        &area,
        &resolve,
    );
    draw_runs(
        sugarloaf,
        &status.centre,
        centre_x,
        right_x,
        &area,
        &resolve,
    );
    draw_runs(sugarloaf, &status.right, right_x, end, &area, &resolve);
}
