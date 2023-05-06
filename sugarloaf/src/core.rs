#[derive(Debug)]
pub struct Sugar {
    pub content: char,
    pub foreground_color: [f32; 4],
    pub background_color: [f32; 4],
}
pub type SugarStack = Vec<Sugar>;
pub type SugarPile = Vec<SugarStack>;

pub fn empty_sugar_pile() -> SugarPile {
    vec![vec![]]
}
