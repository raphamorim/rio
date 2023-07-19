#[derive(Debug)]
pub struct Sugar {
    pub content: char,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
    pub style: Option<SugarStyle>,
    pub decoration: Option<SugarDecoration>,
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
