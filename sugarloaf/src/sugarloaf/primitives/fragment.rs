// use crate::sugarloaf::graphics::SugarGraphic;

#[derive(Debug, Copy, Clone)]
pub struct Fragment {
    pub content: char,
    pub repeated: usize,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
    pub style: FragmentStyle,
    pub decoration: FragmentDecoration,
    pub cursor: FragmentCursor,
    // pub media: Option<TextGraphicFragment>,
}

impl Default for Fragment {
    fn default() -> Self {
        Self {
            content: ' ',
            repeated: 0,
            foreground_color: [0., 0., 0., 0.],
            background_color: [0., 0., 0., 0.],
            style: FragmentStyle::default(),
            decoration: FragmentDecoration::default(),
            cursor: FragmentCursor::default(),
            // media: None,
        }
    }
}

impl PartialEq for Fragment {
    fn eq(&self, other: &Self) -> bool {
        self.content == other.content
            && self.repeated == other.repeated
            && self.foreground_color == other.foreground_color
            && self.background_color == other.background_color
            && self.style == other.style
            && self.decoration == other.decoration
            && self.cursor == other.cursor
    }
}

#[derive(Debug, Default, PartialEq, Copy, Clone)]
pub enum FragmentCursor {
    Block([f32; 4]),
    Caret([f32; 4]),
    Underline([f32; 4]),
    #[default]
    Disabled,
}

#[derive(Debug, Copy, PartialEq, Default, Clone)]
pub enum FragmentDecoration {
    Underline,
    Strikethrough,
    #[default]
    Disabled,
}

#[derive(Debug, PartialEq, Default, Copy, Clone)]
pub struct FragmentStyle {
    pub is_italic: bool,
    pub is_bold: bool,
    pub is_bold_italic: bool,
}