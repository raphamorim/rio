use crate::components::rect::Rect;
use crate::glyph::ab_glyph::PxScale;
use crate::glyph::FontId;
use crate::sugarloaf::constants::{create_sugar_line, LINE_MAX_CHARACTERS};
use crate::sugarloaf::graphics::SugarGraphic;
use serde::Deserialize;
use std::ops::Index;

#[derive(Debug, Default, Copy, Clone)]
pub struct Sugar {
    pub content: char,
    pub repeated: u16,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
    pub style: SugarStyle,
    pub decoration: SugarDecoration,
    pub cursor: Option<SugarCursor>,
    pub custom_decoration: Option<SugarCustomDecoration>,
    pub media: Option<SugarGraphic>,
}

impl PartialEq for Sugar {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
            && self.foreground_color == other.foreground_color
            && self.background_color == other.background_color
            && self.style == other.style
            && self.decoration == other.decoration
    }
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub enum SugarCursorStyle {
    Block,
    Caret,
    Underline,
}

#[derive(Debug, PartialEq, Copy, Clone)]
pub struct SugarCursor {
    pub color: [f32; 4],
    pub style: SugarCursorStyle,
}

#[derive(Debug, Copy, PartialEq, Default, Clone)]
pub enum SugarDecoration {
    Underline,
    Strikethrough,
    #[default]
    Disabled,
}

#[derive(Debug)]
pub struct TextBuilder {
    pub content: String,
    pub font_id: FontId,
    pub scale: PxScale,
    pub color: [f32; 4],
    pub pos_x: f32,
    pub has_initialized: bool,
}

impl TextBuilder {
    pub fn new(font_id: FontId) -> TextBuilder {
        TextBuilder {
            content: String::from(""),
            font_id,
            color: [0., 0., 0., 0.],
            scale: PxScale { x: 0.0, y: 0.0 },
            pos_x: 0.0,
            has_initialized: false,
        }
    }

    #[inline]
    pub fn add(
        &mut self,
        content: &str,
        scale: PxScale,
        color: [f32; 4],
        pos_x: f32,
        font_id: FontId,
    ) {
        // has not initialized yet
        if !self.has_initialized {
            self.scale = scale;
            self.color = color;
            self.pos_x = pos_x;
            self.has_initialized = true;
            self.font_id = font_id;
        }

        self.content += content;
    }

    #[inline]
    pub fn reset(&mut self) {
        // has not initialized yet
        self.content = String::from("");
        self.has_initialized = false;
    }
}

#[derive(Debug)]
pub struct RectBuilder {
    pub color: [f32; 4],
    pub quantity: usize,
    pub pos_x: f32,
    pub pos_y: f32,
    pub width: f32,
    pub height: f32,
}

impl RectBuilder {
    pub fn new(quantity: usize) -> RectBuilder {
        RectBuilder {
            quantity,
            color: [0., 0., 0., 0.],
            pos_x: 0.,
            pos_y: 0.,
            width: 0.,
            height: 0.,
        }
    }

    #[inline]
    pub fn add(
        &mut self,
        pos_x: f32,
        pos_y: f32,
        color: [f32; 4],
        width: f32,
        height: f32,
    ) {
        // RectBuilder is empty
        if self.quantity == 0 {
            self.pos_x = pos_x;
            self.pos_y = pos_y;
            self.color = color;
            self.width = width;
            self.height = height;
        } else {
            self.width += width;
        }

        self.quantity += 1;
    }

    #[inline]
    fn reset(&mut self) {
        self.color = [0., 0., 0., 0.];
        self.pos_x = 0.;
        self.pos_y = 0.;
        self.width = 0.;
        self.height = 0.;
        self.quantity = 0;
    }

    #[inline]
    pub fn build(&mut self) -> Rect {
        let position = [self.pos_x, self.pos_y];
        let color = self.color;
        let size = [self.width, self.height];
        self.reset();
        Rect {
            position,
            color,
            size,
        }
    }
}

#[derive(Debug)]
pub struct RepeatedSugar {
    pub content: Option<char>,
    pub content_str: String,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
    pub style: Option<SugarStyle>,
    pub decoration: Option<SugarCustomDecoration>,
    pub quantity: usize,
    pub pos_x: f32,
    pub pos_y: f32,
    pub reset_on_next: bool,
}

impl RepeatedSugar {
    pub fn new(quantity: usize) -> RepeatedSugar {
        RepeatedSugar {
            content: None,
            content_str: String::from(""),
            foreground_color: [0.0, 0.0, 0.0, 0.0],
            background_color: [0.0, 0.0, 0.0, 0.0],
            style: None,
            decoration: None,
            quantity,
            pos_x: 0.0,
            pos_y: 0.0,
            reset_on_next: false,
        }
    }

    #[inline]
    pub fn set_reset_on_next(&mut self) {
        self.reset_on_next = true;
    }

    #[inline]
    pub fn reset_on_next(&self) -> bool {
        self.reset_on_next
    }

    #[inline]
    pub fn reset(&mut self) {
        self.content = None;
        self.content_str = String::from("");
        self.foreground_color = [0.0, 0.0, 0.0, 0.0];
        self.background_color = [0.0, 0.0, 0.0, 0.0];
        self.quantity = 0;
        self.reset_on_next = false;
    }

    #[inline]
    pub fn set(&mut self, sugar: &Sugar, pos_x: f32, pos_y: f32) {
        self.content = Some(sugar.content);
        self.content_str += &sugar.content.to_string();
        self.foreground_color = sugar.foreground_color;
        self.background_color = sugar.background_color;
        if self.quantity == 0 {
            self.pos_x = pos_x;
            self.pos_y = pos_y;
            self.content_str += &sugar.content.to_string();
        }
        self.quantity += 1;
    }

    #[inline]
    pub fn count(&self) -> usize {
        self.quantity
    }
}

#[derive(Debug, PartialEq, Default, Copy, Clone)]
pub struct SugarStyle {
    pub is_italic: bool,
    pub is_bold: bool,
    pub is_bold_italic: bool,
}

#[derive(Debug, Copy, Clone)]
/// Sugar decoration
/// color, size and position
pub struct SugarCustomDecoration {
    pub relative_position: (f32, f32),
    pub size: (f32, f32),
    pub color: [f32; 4],
}

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

/// Contains a line representation that is hashable and comparable
#[derive(Debug, Copy, Clone)]
pub struct SugarLine {
    // hash: u64,
    // Sized arrays can take up to half of time to execute
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=b3face22f8c64b25803fa213be6a858f
    inner: [Sugar; LINE_MAX_CHARACTERS],
    pub len: usize,
    first_non_default: usize,
    last_non_default: usize,
    default_count: usize,
    default_sugar: Sugar,
}

impl PartialEq for SugarLine {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        if self.is_empty() && other.is_empty() {
            return true;
        }

        if self.len != other.len
            || self.first_non_default != other.first_non_default
            || self.last_non_default != other.last_non_default
            || self.default_count != other.default_count
        {
            return false;
        }

        for i in 0..self.len {
            if self.inner[i] != other.inner[i] {
                return false;
            }
        }

        true
    }
}

impl Default for SugarLine {
    fn default() -> Self {
        Self {
            // hash: 00000000000000,
            last_non_default: 0,
            first_non_default: 0,
            default_count: 0,
            inner: create_sugar_line(),
            default_sugar: Sugar::default(),
            len: 0,
        }
    }
}

impl SugarLine {
    #[inline]
    pub fn insert(&mut self, sugar: Sugar) {
        self.inner[self.len] = sugar;
        if sugar != self.default_sugar {
            if self.first_non_default == 0 {
                self.first_non_default = self.len;
                self.last_non_default = self.len;
            } else {
                self.last_non_default = self.len;
            }

            self.default_count += 1;
        }
        self.len += 1;
    }

    #[inline]
    pub fn insert_empty(&mut self) {
        self.inner[self.len] = self.default_sugar;
        self.len += 1;
    }

    // #[inline]
    // fn compute_hash(&mut self) {
    // 00000000000000
    // 00000000000000 -> first non-default apparison position
    // 00000000000000 -> last non-default apparison position
    // 00000000000000 ->
    // }

    #[inline]
    pub fn is_empty(&self) -> bool {
        // if first digits are zero
        self.last_non_default == 0 && self.default_count == 0
    }

    #[inline]
    pub fn from_vec(&mut self, vector: &Vec<Sugar>) {
        for element in vector.into_iter() {
            self.insert(*element)
        }
    }
}

impl Index<usize> for SugarLine {
    type Output = Sugar;

    fn index(&self, index: usize) -> &Self::Output {
        &self.inner[index]
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    #[test]
    fn test_sugarline_comparisson_exact_match() {
        let line_a = SugarLine::default();
        let line_b = SugarLine::default();

        assert!(line_a.is_empty());
        assert!(line_b.is_empty());
        assert_eq!(line_a, line_b);
    }

    #[test]
    fn test_sugarline_from_vector() {
        let mut line_a = SugarLine::default();
        let vector = vec![
            Sugar {
                content: 't',
                ..Sugar::default()
            },
            Sugar {
                content: 'e',
                ..Sugar::default()
            },
            Sugar {
                content: 'r',
                ..Sugar::default()
            },
            Sugar {
                content: 'm',
                ..Sugar::default()
            },
        ];

        line_a.from_vec(&vector);

        assert!(!line_a.is_empty());
        assert_eq!(line_a.len, 4);
    }

    #[test]
    fn test_sugarline_empty_checks() {
        let mut line_a = SugarLine::default();
        line_a.insert_empty();
        line_a.insert_empty();
        line_a.insert_empty();

        assert!(line_a.is_empty());

        let mut line_a = SugarLine::default();
        line_a.insert(Sugar::default());

        assert!(line_a.is_empty());

        let mut line_a = SugarLine::default();
        line_a.insert(Sugar {
            content: ' ',
            ..Sugar::default()
        });

        assert!(line_a.is_empty());
    }

    #[test]
    fn test_sugarline_comparisson_different_len() {
        let mut line_a = SugarLine::default();
        line_a.insert_empty();
        line_a.insert(Sugar {
            content: 'r',
            ..Sugar::default()
        });
        let line_b = SugarLine::default();

        assert!(!line_a.is_empty());
        assert!(line_b.is_empty());
        assert!(line_a != line_b);

        let mut line_a = SugarLine::default();
        line_a.insert(Sugar {
            content: ' ',
            ..Sugar::default()
        });
        line_a.insert(Sugar {
            content: 'r',
            ..Sugar::default()
        });
        let mut line_b = SugarLine::default();
        line_b.insert(Sugar {
            content: 'r',
            ..Sugar::default()
        });
        line_b.insert(Sugar {
            content: ' ',
            ..Sugar::default()
        });
        line_b.insert(Sugar {
            content: 'i',
            ..Sugar::default()
        });
        line_b.insert(Sugar {
            content: 'o',
            ..Sugar::default()
        });

        assert!(!line_a.is_empty());
        assert!(!line_b.is_empty());
        assert!(line_a != line_b);
    }

    #[test]
    fn test_sugarline_comparisson_different_match_with_same_len() {
        let mut line_a = SugarLine::default();
        line_a.insert(Sugar {
            content: 'o',
            ..Sugar::default()
        });
        line_a.insert(Sugar {
            content: 'i',
            ..Sugar::default()
        });
        line_a.insert(Sugar {
            content: 'r',
            ..Sugar::default()
        });
        let mut line_b = SugarLine::default();
        line_b.insert(Sugar {
            content: 'r',
            ..Sugar::default()
        });
        line_b.insert(Sugar {
            content: 'i',
            ..Sugar::default()
        });
        line_b.insert(Sugar {
            content: 'o',
            ..Sugar::default()
        });

        assert!(!line_a.is_empty());
        assert!(!line_b.is_empty());
        assert!(line_a != line_b);
    }
}
