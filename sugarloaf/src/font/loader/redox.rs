use super::Database;

pub fn load(db: &mut Database) {
    db.load_fonts_dir("/ui/fonts");
}
