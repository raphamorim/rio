use super::Database;

pub fn load(db: &mut Database) {
    db.load_fonts_dir("C:\\Windows\\Fonts\\");

    if let Some(ref system_root) = std::env::var_os("SYSTEMROOT") {
        let system_root_path = std::path::Path::new(system_root);
        db.load_fonts_dir(system_root_path.join("Fonts"));
    } else {
        db.load_fonts_dir("C:\\Windows\\Fonts\\");
    }

    if let Ok(ref home) = std::env::var("USERPROFILE") {
        let home_path = std::path::Path::new(home);
        db.load_fonts_dir(home_path.join("AppData\\Local\\Microsoft\\Windows\\Fonts"));
        db.load_fonts_dir(home_path.join("AppData\\Roaming\\Microsoft\\Windows\\Fonts"));
    }
}
