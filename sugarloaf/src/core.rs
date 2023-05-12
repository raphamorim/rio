#[derive(Debug)]
pub struct Sugar {
    pub content: char,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
}
pub type SugarStack = Vec<Sugar>;
pub type SugarPile = Vec<SugarStack>;

#[derive(Copy, Default, Debug, Clone)]
pub struct SugarloafStyle {
    pub screen_position: (f32, f32),
    pub bounds: (f32, f32),
    pub text_scale: f32,
}

pub fn empty_sugar_pile() -> SugarPile {
    vec![vec![]]
}
