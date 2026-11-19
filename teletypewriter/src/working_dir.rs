//! The configured `working-dir`: a leading `~` expanded, and checked to be
//! a directory before a shell is started in it. One that can't be used
//! falls back to inheriting the current directory, with a warning, instead
//! of failing the spawn (#1088).

use std::path::{is_separator, Path};

/// `dir` with a leading `~`, alone or before a separator, replaced by the
/// home directory. `~user` and a `~` anywhere else are left alone. `None`
/// when there is a `~` to expand but no home directory.
pub fn expand_tilde(dir: &str) -> Option<String> {
    let Some(rest) = dir.strip_prefix('~') else {
        return Some(dir.to_owned());
    };
    if !rest.is_empty() && !rest.starts_with(is_separator) {
        return Some(dir.to_owned());
    }
    // `$HOME`, then the passwd entry on unix; `USERPROFILE` on Windows.
    let home = std::env::home_dir()?;
    Some(format!("{}{rest}", home.to_string_lossy()))
}

/// `dir` expanded, or why a shell can't be started in it.
pub fn check_working_dir(dir: &str) -> Result<String, String> {
    let expanded = expand_tilde(dir)
        .ok_or_else(|| format!("{dir:?} starts with ~ but there is no home directory"))?;
    if Path::new(&expanded).is_dir() {
        Ok(expanded)
    } else {
        Err(format!("{expanded:?} is not a directory"))
    }
}

/// The directory to start a shell in: `dir` expanded, or `None` (inherit
/// the current directory) when it is unset or can't be used, with a
/// warning in the latter case.
pub fn resolve_working_dir(dir: &Option<String>) -> Option<String> {
    match check_working_dir(dir.as_deref()?) {
        Ok(dir) => Some(dir),
        Err(why) => {
            tracing::warn!("working-dir {why}; inheriting the current directory");
            None
        }
    }
}

/// Whether this process runs in a Flatpak sandbox, where shells are
/// started on the host, which may see directories the sandbox can't.
pub fn in_flatpak() -> bool {
    cfg!(target_os = "linux") && Path::new("/.flatpak-info").exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> String {
        std::env::home_dir()
            .expect("tests need a home directory")
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn unset_stays_unset() {
        assert_eq!(resolve_working_dir(&None), None);
    }

    #[test]
    fn a_tilde_is_the_home_directory() {
        assert_eq!(expand_tilde("~"), Some(home()));
        assert_eq!(resolve_working_dir(&Some("~".into())), Some(home()));
    }

    #[test]
    fn a_tilde_before_a_separator_is_under_home() {
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            expand_tilde(&format!("~{sep}projects")),
            Some(format!("{}{sep}projects", home()))
        );
    }

    #[test]
    fn only_a_leading_tilde_alone_is_expanded() {
        for dir in ["/tmp/~x", "~user", "~user/x", "a~"] {
            assert_eq!(expand_tilde(dir), Some(dir.to_owned()), "{dir}");
        }
    }

    #[test]
    fn an_existing_directory_passes_through() {
        let dir = std::env::temp_dir().to_string_lossy().into_owned();
        assert_eq!(resolve_working_dir(&Some(dir.clone())), Some(dir));
    }

    #[test]
    fn a_missing_directory_falls_back_to_inheriting() {
        let missing = std::env::temp_dir()
            .join("rio-working-dir-that-does-not-exist")
            .to_string_lossy()
            .into_owned();
        assert!(check_working_dir(&missing)
            .unwrap_err()
            .contains("is not a directory"));
        assert_eq!(resolve_working_dir(&Some(missing)), None);
    }

    #[test]
    fn a_file_is_not_a_working_directory() {
        let file = std::env::temp_dir().join("rio-working-dir-a-file");
        std::fs::write(&file, b"").unwrap();
        let file = file.to_string_lossy().into_owned();
        assert_eq!(resolve_working_dir(&Some(file.clone())), None);
        std::fs::remove_file(file).unwrap();
    }
}
