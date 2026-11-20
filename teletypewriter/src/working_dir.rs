//! The configured `working-dir`, resolved as Ghostty resolves its
//! `working-directory`: `home` and `inherit` are special values, a
//! leading `~` is the home directory, and a directory a shell can't be
//! started in falls back to inheriting the current one, with a warning,
//! instead of failing the spawn (#1088).

use std::path::{is_separator, Path, PathBuf};

/// The home directory: `$HOME` (unless empty), then the passwd entry on
/// unix; the profile on Windows.
fn home_dir() -> Option<PathBuf> {
    std::env::home_dir()
}

/// `dir` with a leading `~`, alone or before separators, replaced by
/// `home`, joined as a path so a home of `/` doesn't give `//x`. `~user`
/// and a `~` anywhere else are left alone. Fails only when there is a `~`
/// to expand and no usable home: none, or one that isn't UTF-8, which the
/// expanded path couldn't carry.
fn expand_tilde(
    dir: &str,
    home: impl FnOnce() -> Option<PathBuf>,
) -> Result<String, String> {
    let Some(rest) = dir.strip_prefix('~') else {
        return Ok(dir.to_owned());
    };
    if !rest.is_empty() && !rest.starts_with(is_separator) {
        return Ok(dir.to_owned());
    }
    let home = home().ok_or_else(|| "there is no home directory".to_owned())?;
    let rest = rest.trim_start_matches(is_separator);
    let path = if rest.is_empty() {
        home
    } else {
        home.join(rest)
    };
    path.into_os_string()
        .into_string()
        .map_err(|path| format!("the home directory {path:?} isn't UTF-8"))
}

/// The directory `dir` names, before checking it: `inherit` is none (the
/// current directory), `home` the home directory, and anything else is
/// a path with a leading `~` expanded.
fn expand_working_dir(dir: &str) -> Result<Option<String>, String> {
    let expanded = match dir {
        "inherit" => return Ok(None),
        "home" => expand_tilde("~", home_dir),
        _ => expand_tilde(dir, home_dir),
    };
    expanded.map(Some).map_err(|why| format!("{dir:?}: {why}"))
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
        assert_eq!(expand_tilde("~", home_dir), Ok(home()));
        assert_eq!(resolve_working_dir(&Some("~".into())), Some(home()));
    }

    #[test]
    fn a_tilde_before_a_separator_is_under_home() {
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            expand_tilde(&format!("~{sep}projects"), home_dir),
            Ok(format!("{}{sep}projects", home()))
        );
    }

    #[test]
    fn only_a_leading_tilde_alone_is_expanded() {
        for dir in ["/tmp/~x", "~user", "~user/x", "a~"] {
            assert_eq!(expand_tilde(dir, home_dir), Ok(dir.to_owned()), "{dir}");
        }
    }

    /// Joined as paths: a root home gives `/x`, not `//x`, and repeated
    /// or trailing separators after the `~` don't double up.
    #[cfg(unix)]
    #[test]
    fn the_home_directory_is_joined_as_a_path() {
        let root = || Some(PathBuf::from("/"));
        assert_eq!(expand_tilde("~/x", root), Ok("/x".to_owned()));
        assert_eq!(expand_tilde("~", root), Ok("/".to_owned()));
        let home = || Some(PathBuf::from("/home/me"));
        assert_eq!(expand_tilde("~//x/", home), Ok("/home/me/x/".to_owned()));
        assert_eq!(expand_tilde("~/", home), Ok("/home/me".to_owned()));
    }

    #[test]
    fn a_tilde_without_a_home_is_an_error() {
        let none = || None;
        assert!(expand_tilde("~/x", none)
            .unwrap_err()
            .contains("no home directory"));
        // Nothing to expand, so no home is needed.
        assert_eq!(expand_tilde("/x", none), Ok("/x".to_owned()));
    }

    /// A home that isn't UTF-8 can't be carried in the expanded path, so
    /// it's reported instead of turned into a different path.
    #[cfg(unix)]
    #[test]
    fn a_home_that_isnt_utf8_is_an_error() {
        use std::os::unix::ffi::OsStrExt;
        let odd = || Some(PathBuf::from(std::ffi::OsStr::from_bytes(b"/home/\xff")));
        assert!(expand_tilde("~", odd).unwrap_err().contains("isn't UTF-8"));
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
