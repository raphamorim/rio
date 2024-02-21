use super::Database;

pub fn load(db: &mut Database) {
    load_fontconfig(db);

    db.load_fonts_dir("/usr/share/fonts/");
    db.load_fonts_dir("/usr/local/share/fonts/");

    if let Ok(ref home) = std::env::var("HOME") {
        let home_path = std::path::Path::new(home);
        db.load_fonts_dir(home_path.join(".fonts"));
        db.load_fonts_dir(home_path.join(".local/share/fonts"));
    }
}

fn load_fontconfig(database: &mut Database) {
    use std::path::Path;

    let mut fontconfig = fontconfig_parser::FontConfig::default();
    let home = std::env::var("HOME");

    if let Ok(ref config_file) = std::env::var("FONTCONFIG_FILE") {
        let _ = fontconfig.merge_config(Path::new(config_file));
    } else {
        let xdg_config_home = if let Ok(val) = std::env::var("XDG_CONFIG_HOME") {
            Some(val.into())
        } else if let Ok(ref home) = home {
            // according to https://specifications.freedesktop.org/basedir-spec/basedir-spec-latest.html
            // $XDG_CONFIG_HOME should default to $HOME/.config if not set
            Some(Path::new(home).join(".config"))
        } else {
            None
        };

        let read_global = match xdg_config_home {
            Some(p) => fontconfig
                .merge_config(&p.join("fontconfig/fonts.conf"))
                .is_err(),
            None => true,
        };

        if read_global {
            let _ = fontconfig.merge_config(Path::new("/etc/fonts/local.conf"));
        }
        let _ = fontconfig.merge_config(Path::new("/etc/fonts/fonts.conf"));
    }

    for fontconfig_parser::Alias {
        alias,
        default,
        prefer,
        accept,
    } in fontconfig.aliases
    {
        let name = prefer
            .first()
            .or_else(|| accept.first())
            .or_else(|| default.first());

        if let Some(name) = name {
            match alias.to_lowercase().as_str() {
                "serif" => database.set_serif_family(name),
                "sans-serif" => database.set_sans_serif_family(name),
                "sans serif" => database.set_sans_serif_family(name),
                "monospace" => database.set_monospace_family(name),
                "cursive" => database.set_cursive_family(name),
                "fantasy" => database.set_fantasy_family(name),
                _ => {}
            }
        }
    }

    for dir in fontconfig.dirs {
        let path = if dir.path.starts_with("~") {
            if let Ok(ref home) = home {
                Path::new(home).join(dir.path.strip_prefix("~").unwrap())
            } else {
                continue;
            }
        } else {
            dir.path
        };
        database.load_fonts_dir(path);
    }
}
