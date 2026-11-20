//! The configured `working-dir`, resolved as Ghostty resolves its
//! `working-directory`: `home` and `inherit` are special values, a
//! leading `~` is the home directory, and a directory a shell can't be
//! started in falls back to inheriting the current one, with a warning,
//! instead of failing the spawn (#1088).

use std::path::{is_separator, Path};

/// `dir` with a leading `~`, alone or before a separator, replaced by the
/// home directory. `~user` and a `~` anywhere else are left alone. `None`
/// when there is a `~` to expand but no home directory.
fn expand_tilde(dir: &str) -> Option<String> {
    let Some(rest) = dir.strip_prefix('~') else {
        return Some(dir.to_owned());
    };
    if !rest.is_empty() && !rest.starts_with(is_separator) {
        return Some(dir.to_owned());
    }
    // `$HOME`, then the passwd entry on unix; the profile on Windows.
    let home = std::env::home_dir()?;
    Some(format!("{}{rest}", home.to_string_lossy()))
}

/// The directory `dir` names, before checking it: `inherit` is none (the
/// current directory), `home` the home directory, and anything else is
/// a path with a leading `~` expanded.
fn expand_working_dir(dir: &str) -> Result<Option<String>, String> {
    match dir {
        "inherit" => Ok(None),
        "home" => std::env::home_dir()
            .map(|home| Some(home.to_string_lossy().into_owned()))
            .ok_or_else(|| "home: there is no home directory".to_owned()),
        _ => expand_tilde(dir).map(Some).ok_or_else(|| {
            format!("{dir:?} starts with ~ but there is no home directory")
        }),
    }
}

/// The directory to start a shell in, if `dir` names one other than the
/// current directory, or why a shell can't be started in it: it has to
/// open, as Ghostty requires access and WezTerm a readable directory, so
/// one we can't enter (after `sudo -i`, say) doesn't fail the spawn.
pub fn check_working_dir(dir: &str) -> Result<Option<String>, String> {
    let Some(dir) = expand_working_dir(dir)? else {
        return Ok(None);
    };
    match std::fs::read_dir(Path::new(&dir)) {
        Ok(_) => Ok(Some(dir)),
        Err(err) => Err(format!("{dir:?} can't be opened: {err}")),
    }
}

/// The directory to start a shell in: `dir` resolved, or `None` (inherit
/// the current directory) when it is unset, `inherit`, or can't be used,
/// with a warning in the last case.
pub fn resolve_working_dir(dir: &Option<String>) -> Option<String> {
    match check_working_dir(dir.as_deref()?) {
        Ok(dir) => dir,
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

/// [`resolve_working_dir`] for a shell started on the host from a Flatpak
/// sandbox. The sandbox can't check the directory itself: some host paths
/// are hidden from it whatever its permissions. So, as Ghostty does, the
/// host starts `/bin/sh -c :` there, and the directory is used only if
/// that works.
#[cfg(target_os = "linux")]
pub fn resolve_host_working_dir(dir: &Option<String>) -> Option<String> {
    use std::process::{Command, Stdio};

    let checked = expand_working_dir(dir.as_deref()?).and_then(|dir| {
        let Some(dir) = dir else { return Ok(None) };
        let status = Command::new("flatpak-spawn")
            .arg("--host")
            .arg(format!("--directory={dir}"))
            .args(["/bin/sh", "-c", ":"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|err| format!("{dir:?} couldn't be checked on the host: {err}"))?;
        if status.success() {
            Ok(Some(dir))
        } else {
            Err(format!("{dir:?} can't be entered on the host"))
        }
    });
    match checked {
        Ok(dir) => dir,
        Err(why) => {
            tracing::warn!("working-dir {why}; inheriting the current directory");
            None
        }
    }
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
    fn unset_and_inherit_inherit() {
        assert_eq!(resolve_working_dir(&None), None);
        assert_eq!(check_working_dir("inherit"), Ok(None));
    }

    #[test]
    fn home_and_a_tilde_are_the_home_directory() {
        assert_eq!(check_working_dir("home"), Ok(Some(home())));
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
            .contains("can't be opened"));
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

    /// A directory we can't enter would fail the spawn as surely as a
    /// missing one. (Root can enter anything, so this proves nothing there.)
    #[cfg(unix)]
    #[test]
    fn a_directory_we_cant_enter_falls_back_to_inheriting() {
        use std::os::unix::fs::PermissionsExt;
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = std::env::temp_dir().join("rio-working-dir-locked");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o000)).unwrap();
        let path = dir.to_string_lossy().into_owned();
        let checked = check_working_dir(&path);
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::remove_dir(&dir).unwrap();
        assert!(checked.unwrap_err().contains("can't be opened"));
    }
}
