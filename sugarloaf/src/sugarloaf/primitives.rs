use std::mem::MaybeUninit;
use crate::components::rect::Rect;
use crate::glyph::ab_glyph::PxScale;
use crate::glyph::FontId;
use crate::sugarloaf::graphics::SugarGraphic;
use serde::Deserialize;

#[derive(Debug, Default, Copy, Clone)]
pub struct Sugar {
    pub content: char,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
    pub style: Option<SugarStyle>,
    pub decoration: Option<SugarDecoration>,
    pub cursor: Option<SugarCursor>,
    pub custom_decoration: Option<SugarCustomDecoration>,
    pub media: Option<SugarGraphic>,
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

#[derive(Debug, Copy, Clone)]
pub enum SugarDecoration {
    Underline,
    Strikethrough,
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

#[derive(Debug, Copy, Clone)]
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

pub type SugarStack = Vec<Sugar>;

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

const LINE_MAX_CHARACTERS: usize = 400;

/// Contains a line representation that is hashable and comparable
#[derive(Debug, Copy, Clone)]
pub struct SugarLine {
    hash: u64,
    // Sized arrays can take up to half of time to execute
    // https://play.rust-lang.org/?version=stable&mode=debug&edition=2018&gist=b3face22f8c64b25803fa213be6a858f
    inner: [Sugar; LINE_MAX_CHARACTERS],
    pub len: usize,
}

impl Default for SugarLine {
    fn default() -> Self {
        let iter = std::iter::repeat(Sugar::default()).take(LINE_MAX_CHARACTERS);

        let inner = {
            // Create an array of uninitialized values.
            let mut array: [MaybeUninit<Sugar>; LINE_MAX_CHARACTERS] = unsafe { MaybeUninit::uninit().assume_init() };

            for (i, element) in array.iter_mut().enumerate() {
                *element = MaybeUninit::new(Sugar::default());
            }

            unsafe { std::mem::transmute::<_, [Sugar; LINE_MAX_CHARACTERS]>(array) }
        };

        Self {
            hash: 0,
            inner,
            len: 0,
        }
    }
}

impl SugarLine {
    #[inline]
    pub fn insert(&mut self, sugar: Sugar) {
        self.inner[self.len] = sugar;
        self.len += 1;
    }

    #[inline]
    pub fn is_empty_line() -> bool{
        false
    } 
}
