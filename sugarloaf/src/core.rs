use std::iter::Peekable;

use crate::components::rect::Rect;
use crate::graphics::SugarGraphic;
use crate::sugarloaf::TextInfo;
use ab_glyph::Point;
use serde::Deserialize;
use unicode_width::UnicodeWidthChar;

#[derive(Debug, PartialEq, Default)]
pub struct Sugar {
    pub content: char,
    pub fg_color: [f32; 4],
    pub bg_color: [f32; 4],
    pub style: SugarStyle,
    pub decoration: Option<SugarDecoration>,
    pub media: Option<SugarGraphic>,
}

#[derive(Debug)]
pub struct Text {
    pub content: char,
    pub quantity: usize,

    pub fg_color: [f32; 4],
    pub bg_color: [f32; 4],

    pub style: SugarStyle,
    pub decoration: Option<SugarDecoration>,
    pub media: Option<SugarGraphic>,

    pub pos: Point,
}

impl Text {
    pub fn build_from(
        iterator: &mut Peekable<impl Iterator<Item = Sugar>>,
        pos: &Point,
    ) -> Self {
        let sugar = iterator.next().unwrap();

        let Sugar {
            content,
            fg_color,
            bg_color,
            style,
            decoration,
            media,
        } = sugar;

        let quantity = if decoration.is_some() || media.is_some() {
            1
        } else {
            let mut counter = 1;

            while iterator
                .next_if(|next_sugar| {
                    next_sugar.content == sugar.content
                        && next_sugar.fg_color == sugar.fg_color
                        && next_sugar.bg_color == sugar.bg_color
                        && next_sugar.decoration == sugar.decoration
                        && next_sugar.media == sugar.media
                })
                .is_some()
            {
                counter += 1;
            }

            counter
        };

        Self {
            content,
            quantity,
            fg_color,
            bg_color,
            style,
            decoration,
            media,
            pos: *pos,
        }
    }

    /// Returns the display width of the text.
    pub fn width(&self) -> usize {
        self.content.width().unwrap_or(1) * self.quantity
    }
}

impl From<(&Text, TextInfo)> for crate::components::text::OwnedText {
    fn from((text, info): (&Text, TextInfo)) -> Self {
        let text_content = String::from(text.content).repeat(text.quantity);

        Self {
            text: text_content,
            scale: info.px_scale,
            font_id: info.font_id,
            extra: crate::components::text::Extra {
                color: text.fg_color,
                z: 0.0,
            },
        }
    }
}

#[derive(Debug)]
pub struct RectBuilder {
    pub sugarwidth: f32,
    pub sugarheight: f32,
    pub scale: f32,
}

impl BuildRectFor<&Text> for RectBuilder {
    fn build_for(&self, text: &Text) -> Vec<Rect> {
        let text_rect = {
            let pos = Point {
                x: text.pos.x / self.scale,
                y: (text.pos.y - self.sugarheight) / self.scale,
            };

            Rect {
                position: [pos.x, pos.y],
                color: text.bg_color,
                size: [text.width() as f32 * self.sugarwidth, self.sugarheight],
            }
        };

        let mut rects = vec![text_rect];

        if let Some(decoration) = text.decoration {
            let text_char_width = text.content.width().unwrap_or(1);
            let pos = Point {
                x: (text.pos.x + text_char_width as f32 * decoration.relative_position.0)
                    / self.scale,
                y: (text.pos.y / self.scale) + decoration.relative_position.1,
            };

            rects.push(Rect {
                position: [pos.x, pos.y],
                color: decoration.color,
                size: [
                    text.width() as f32 * self.sugarwidth * decoration.size.0,
                    self.sugarheight * decoration.size.1,
                ],
            });
        }

        rects
    }
}

pub trait BuildRectFor<T> {
    fn build_for(&self, t: T) -> Vec<Rect>;
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SugarStyle {
    pub italic: bool,
    pub bold: bool,
}

#[derive(Debug, Copy, Clone, PartialEq)]
/// Sugar decoration
/// color, size and position
pub struct SugarDecoration {
    // pub position: SugarDecorationPosition,
    pub relative_position: (f32, f32),
    pub size: (f32, f32),
    pub color: [f32; 4],
}
pub type SugarDecorationPosition = (SugarDecorationPositionX, SugarDecorationPositionY);

#[derive(Debug, Copy, Clone)]
/// Sugar decoration position in x axis
pub enum SugarDecorationPositionX {
    Left(f32),
    Right(f32),
}

#[derive(Debug, Copy, Clone)]
/// Sugar decoration position in y axis
pub enum SugarDecorationPositionY {
    Top(f32),
    Middle(f32),
    Bottom(f32),
}

pub type SugarStack = Vec<Sugar>;
pub type SugarPile = Vec<SugarStack>;

#[derive(Copy, Default, Debug, Clone)]
pub struct SugarloafStyle {
    pub screen_position: (f32, f32),
    pub line_height: f32,
    pub text_scale: f32,
}

#[derive(Default, Clone, Deserialize, Debug, PartialEq)]
pub struct ImageProperties {
    #[serde(default = "String::default")]
    pub path: String,
    #[serde(default = "f32::default")]
    pub width: f32,
    #[serde(default = "f32::default")]
    pub height: f32,
    #[serde(default = "f32::default")]
    pub x: f32,
    #[serde(default = "f32::default")]
    pub y: f32,
}
