// use crate::SugarKind;
// use crate::Sugar;

#[derive(Default, PartialEq, Clone)]
pub struct Text {
    pub position: (f32, f32),
    pub content: String,
    pub font_id: usize,
    pub font_size: f32,
    pub color: [f32; 4],
    pub single_line: bool,
}

impl Text {
    #[inline]
    pub fn new(
        position: (f32, f32),
        content: String,
        font_id: usize,
        font_size: f32,
        color: [f32; 4],
        single_line: bool,
    )-> Self  {
        Self {
            position,
            content,
            font_id,
            font_size,
            color,
            single_line,
        }
    }
}

// impl Sugar for Text {
//     // fn should_update(&self, other: &Self) -> bool {
//     //     self == other
//     // }
//     fn kind(&self) -> SugarKind {
//         SugarKind::Text
//     }
// }