use super::Database;

pub fn load(db: &mut Database) {
    db.load_fonts_dir("C:\\Windows\\Fonts\\");

    if let Ok(ref home) = std::env::var("USERPROFILE") {
        let home_path = std::path::Path::new(home);
        db.load_fonts_dir(home_path.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
        db.load_fonts_dir(home_path.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
    }
}
