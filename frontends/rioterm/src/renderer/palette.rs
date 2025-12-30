use rio_backend::config::colors::Colors;
use rio_backend::sugarloaf::{Object, Quad, RichText};

#[inline]
pub fn draw_command_palette(
    objects: &mut Vec<Object>,
    rich_text_id: usize,
    colors: &Colors,
    dimensions: (f32, f32, f32),
) {
    let (width, height, scale) = dimensions;

    // Calculate palette dimensions
    let palette_width = (width * 0.6).min(600.0);
    let palette_height = 300.0;
    let position_x = (width - palette_width) / 2.0;
    let position_y = 40.0 / scale;

    // Draw semi-transparent background overlay
    objects.push(Object::Quad(Quad {
        position: [0.0, 0.0],
        color: [0.0, 0.0, 0.0, 0.5],
        size: [width, height],
        ..Quad::default()
    }));

    // Draw palette background
    objects.push(Object::Quad(Quad {
        position: [position_x, position_y],
        color: colors.bar,
        size: [palette_width, palette_height],
        ..Quad::default()
    }));

    // Draw text (input and results)
    objects.push(Object::RichText(RichText {
        id: rich_text_id,
        position: [position_x + 10., position_y + 5.],
        lines: None,
    }));
}
