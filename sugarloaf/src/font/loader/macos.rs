use super::Database;

pub fn load(db: &mut Database) {
    db.load_fonts_dir("/Library/Fonts");
    db.load_fonts_dir("/System/Library/Fonts");
    // Downloadable fonts, location varies on major macOS releases
    if let Ok(dir) = std::fs::read_dir("/System/Library/AssetsV2") {
        for entry in dir {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("com_apple_MobileAsset_Font")
            {
                db.load_fonts_dir(entry.path());
            }
        }
    }
    db.load_fonts_dir("/Network/Library/Fonts");

    if let Ok(ref home) = std::env::var("HOME") {
        let home_path = std::path::Path::new(home);
        db.load_fonts_dir(home_path.join("Library/Fonts"));
    }
}
