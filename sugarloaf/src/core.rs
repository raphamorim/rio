use crate::components::rect::Rect;
use serde::Deserialize;

#[derive(Debug)]
pub struct Sugar {
    pub content: char,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
    pub style: Option<SugarStyle>,
    pub decoration: Option<SugarDecoration>,
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

    // scaled_rect_pos_x,
    //             scaled_rect_pos_y,
    //             bg_color,
    //             width_bound,
    //             self.layout.sugarheight,

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
    pub decoration: Option<SugarDecoration>,
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

#[derive(Debug)]
pub struct SugarStyle {
    pub is_italic: bool,
    pub is_bold: bool,
    pub is_bold_italic: bool,
}

#[derive(Debug, Copy, Clone)]
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
