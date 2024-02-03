// Sugarstack as hashable content
// inner Sugar{}, Sugar{}, ... -> hash 12039120

pub enum Diff {
    NoChange,
    LinesChanged(i16),
    ColumunsChanged(i16),
    InsertedCharacter,
    DeletedCharacter,
}

pub struct SugarloafTree {
    current_content: Option<String>,
    next_content: Option<String>,
    lines: u16,
    columns: u16,
    cursor_position: u16,
}
