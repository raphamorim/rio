use crate::components::rect::Rect;
use crate::font::{FONT_ID_BOLD, FONT_ID_BOLD_ITALIC, FONT_ID_ITALIC, FONT_ID_REGULAR};
use crate::glyph::ab_glyph::PxScale;
use crate::glyph::{FontId, OwnedSection, OwnedText};
use crate::graphics::SugarGraphic;
use ab_glyph::Point;
use fnv::FnvHashMap;
use serde::Deserialize;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

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

    pub font_id: FontId,
    pub fg_color: [f32; 4],
    pub bg_color: [f32; 4],

    pub style: SugarStyle,
    pub decoration: Option<SugarDecoration>,
    pub media: Option<SugarGraphic>,

    pub pos: Point,
}

impl Text {
    pub fn build_from(iterator: &mut impl Iterator<Item = Sugar>, pos: &Point) -> Self {
        let sugar = iterator.next().unwrap();

        let font_id = FontId::from(sugar.style);

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
            iterator
                .take_while(|&next_sugar| next_sugar == sugar)
                .count()
        };

        Self {
            content,
            quantity,
            font_id,
            fg_color,
            bg_color,
            style,
            decoration,
            media,
            pos: pos.clone(),
        }
    }

    /// Returns the display width of the text.
    pub fn width(&self) -> usize {
        self.content.width().unwrap_or(1) * self.quantity
    }

    // pub fn new(font_id: FontId) -> TextBuilder {
    //     TextBuilder {
    //         content: String::from(""),
    //         font_id,
    //         fg_color: [0., 0., 0., 0.],
    //         scale: PxScale { x: 0.0, y: 0.0 },
    //         pos_x: 0.0,
    //         has_initialized: false,
    //     }
    // }

    // #[inline]
    // pub fn add(
    //     &mut self,
    //     content: &str,
    //     scale: PxScale,
    //     color: [f32; 4],
    //     pos_x: f32,
    //     font_id: FontId,
    // ) {
    //     // has not initialized yet
    //     if !self.has_initialized {
    //         self.scale = scale;
    //         self.fg_color = color;
    //         self.pos_x = pos_x;
    //         self.has_initialized = true;
    //         self.font_id = font_id;
    //     }

    //     self.content += content;
    // }

    // #[inline]
    // pub fn reset(&mut self) {
    //     // has not initialized yet
    //     self.content = String::from("");
    //     self.has_initialized = false;
    // }
}

impl From<(&Text, PxScale)> for crate::components::text::OwnedText {
    fn from((text, scale): (&Text, PxScale)) -> Self {
        let text_content = String::from(text.content).repeat(text.quantity);

        Self {
            text: text_content,
            scale,
            font_id: text.font_id,
            extra: crate::components::text::Extra {
                color: text.fg_color,
                z: 0.0,
            },
        }
    }
}

#[derive(Debug)]
pub struct RectBuilder {
    pub sugar_char_width: f32,
    pub sugarheight: f32,
    pub scale: f32,
}

impl BuildRectFor<&Text> for RectBuilder {
    fn build_for(&self, text: &Text) -> Vec<Rect> {
        let text_rect = {
            let pos = Point {
                x: text.pos.x / self.scale,
                y: text.pos.y / self.scale,
            };

            Rect {
                position: [pos.x, pos.y],
                color: text.bg_color,
                size: [
                    text.width() as f32 * self.sugar_char_width,
                    self.sugarheight,
                ],
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
                    self.sugar_char_width * text.width() as f32 * decoration.size.0,
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

// #[derive(Debug)]
// pub struct RepeatedSugar {
//     pub content: Option<char>,
//     pub content_str: String,
//     pub foreground_color: [f32; 4],
//     pub background_color: [f32; 4],
//     pub style: Option<SugarStyle>,
//     pub decoration: Option<SugarDecoration>,
//     pub quantity: usize,
//     pub pos_x: f32,
//     pub pos_y: f32,
//     pub reset_on_next: bool,
// }

// impl RepeatedSugar {
//     pub fn new(quantity: usize) -> RepeatedSugar {
//         RepeatedSugar {
//             content: None,
//             content_str: String::from(""),
//             foreground_color: [0.0, 0.0, 0.0, 0.0],
//             background_color: [0.0, 0.0, 0.0, 0.0],
//             style: None,
//             decoration: None,
//             quantity,
//             pos_x: 0.0,
//             pos_y: 0.0,
//             reset_on_next: false,
//         }
//     }

//     #[inline]
//     pub fn set_reset_on_next(&mut self) {
//         self.reset_on_next = true;
//     }

//     #[inline]
//     pub fn reset_on_next(&self) -> bool {
//         self.reset_on_next
//     }

//     #[inline]
//     pub fn reset(&mut self) {
//         self.content = None;
//         self.content_str = String::from("");
//         self.foreground_color = [0.0, 0.0, 0.0, 0.0];
//         self.background_color = [0.0, 0.0, 0.0, 0.0];
//         self.quantity = 0;
//         self.reset_on_next = false;
//     }

//     #[inline]
//     pub fn set(&mut self, sugar: &Sugar, pos_x: f32, pos_y: f32) {
//         self.content = Some(sugar.content);
//         self.content_str += &sugar.content.to_string();
//         self.foreground_color = sugar.foreground_color;
//         self.background_color = sugar.background_color;
//         if self.quantity == 0 {
//             self.pos_x = pos_x;
//             self.pos_y = pos_y;
//             self.content_str += &sugar.content.to_string();
//         }
//         self.quantity += 1;
//     }

//     #[inline]
//     pub fn count(&self) -> usize {
//         self.quantity
//     }
// }

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SugarStyle {
    pub italic: bool,
    pub bold: bool,
}

impl From<SugarStyle> for FontId {
    fn from(style: SugarStyle) -> Self {
        if style.italic && style.bold {
            FontId(FONT_ID_BOLD_ITALIC)
        } else if style.italic {
            FontId(FONT_ID_ITALIC)
        } else if style.bold {
            FontId(FONT_ID_BOLD)
        } else {
            FontId(FONT_ID_REGULAR)
        }
    }
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

pub fn empty_sugar_pile() -> SugarPile {
    vec![vec![]]
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
