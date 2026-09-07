//! Automatic shell integration.
//!
//! Rio embeds small per-shell scripts that report the working
//! directory to the terminal (OSC 7, `kitty-shell-cwd://` flavor) on
//! every prompt and directory change. At spawn time the scripts are
//! written under the user's cache directory (once per rio version)
//! and the child's environment is pointed at them: `ZDOTDIR` for zsh
//! (the user's value is preserved in `RIO_ZSH_ZDOTDIR` and restored
//! before any of their configuration runs) and an `XDG_DATA_DIRS`
//! prepend for fish, whose `vendor_conf.d` loads from there. Shells
//! without an environment-only hook (bash needs its argv rewritten)
//! are left untouched.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const ZSH_ZSHENV: &str = include_str!("zshenv.zsh");
const ZSH_INTEGRATION: &str = include_str!("rio-integration.zsh");
const FISH_INTEGRATION: &str = include_str!("rio.fish");

/// Extra environment for the pane about to spawn `shell_program`
/// (falling back to `$SHELL`, mirroring the PTY spawn itself). Empty
/// when the shell has no environment-only integration hook or the
/// scripts could not be written.
pub fn spawn_env(shell_program: Option<&str>) -> Vec<(String, String)> {
    let program = match shell_program {
        Some(program) if !program.is_empty() => program.to_string(),
        _ => std::env::var("SHELL").unwrap_or_default(),
    };
    let shell_name = Path::new(&program)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let Some(dir) = integration_dir() else {
        return Vec::new();
    };
    env_pairs(
        &shell_name,
        dir,
        std::env::var("ZDOTDIR").ok(),
        std::env::var("XDG_DATA_DIRS").ok(),
    )
}

/// The environment that makes `shell_name` load the scripts under
/// `dir`. Pure so the mapping is testable without touching the real
/// process environment or cache directory.
fn env_pairs(
    shell_name: &str,
    dir: &Path,
    current_zdotdir: Option<String>,
    current_xdg_data_dirs: Option<String>,
) -> Vec<(String, String)> {
    match shell_name {
        "zsh" => {
            let mut envs = Vec::new();
            if let Some(old) = current_zdotdir {
                envs.push(("RIO_ZSH_ZDOTDIR".to_string(), old));
            }
            envs.push((
                "ZDOTDIR".to_string(),
                dir.join("zsh").to_string_lossy().to_string(),
            ));
            envs
        }
        "fish" => {
            let data_dir = dir.join("data").to_string_lossy().to_string();
            let dirs = match current_xdg_data_dirs {
                Some(existing) if !existing.is_empty() => {
                    format!("{data_dir}:{existing}")
                }
                // The XDG spec default applies when the variable is
                // unset; spell it out so prepending does not hide it.
                _ => format!("{data_dir}:/usr/local/share:/usr/share"),
            };
            vec![("XDG_DATA_DIRS".to_string(), dirs)]
        }
        _ => Vec::new(),
    }
}

/// The on-disk script directory, materialized once per process. Keyed
/// by rio's version so upgrades rewrite stale scripts and downgrades
/// never read newer ones.
fn integration_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let base = dirs::cache_dir()?
            .join("rio")
            .join(format!("shell-integration-{}", env!("CARGO_PKG_VERSION")));
        if let Err(err) = materialize(&base) {
            tracing::warn!("shell integration scripts not written: {err}");
            return None;
        }
        Some(base)
    })
    .as_deref()
}

/// Write the embedded scripts under `base`. Rewrites only files whose
/// content differs, so concurrent rio processes racing here write the
/// same bytes and the result is valid either way.
fn materialize(base: &Path) -> std::io::Result<()> {
    let zsh = base.join("zsh");
    std::fs::create_dir_all(&zsh)?;
    write_if_changed(&zsh.join(".zshenv"), ZSH_ZSHENV)?;
    write_if_changed(&zsh.join("rio-integration.zsh"), ZSH_INTEGRATION)?;

    let fish = base.join("data").join("fish").join("vendor_conf.d");
    std::fs::create_dir_all(&fish)?;
    write_if_changed(&fish.join("rio.fish"), FISH_INTEGRATION)?;
    Ok(())
}

fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path)
        .map(|current| current == content)
        .unwrap_or(false)
    {
        return Ok(());
    }
    std::fs::write(path, content)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn env_pairs_per_shell() {
        let dir = Path::new("/cache/rio/shell-integration-1.0.0");

        let zsh = env_pairs("zsh", dir, None, None);
        assert_eq!(
            zsh,
            vec![(
                "ZDOTDIR".to_string(),
                "/cache/rio/shell-integration-1.0.0/zsh".to_string()
            )]
        );

        // A user ZDOTDIR is preserved for the .zshenv chain to restore.
        let zsh = env_pairs("zsh", dir, Some("/home/u/.config/zsh".into()), None);
        assert_eq!(
            zsh[0],
            (
                "RIO_ZSH_ZDOTDIR".to_string(),
                "/home/u/.config/zsh".to_string()
            )
        );
        assert_eq!(zsh[1].0, "ZDOTDIR");

        let fish = env_pairs("fish", dir, None, Some("/usr/share".into()));
        assert_eq!(
            fish,
            vec![(
                "XDG_DATA_DIRS".to_string(),
                "/cache/rio/shell-integration-1.0.0/data:/usr/share".to_string()
            )]
        );

        // Unset XDG_DATA_DIRS keeps the spec default visible.
        let fish = env_pairs("fish", dir, None, None);
        assert!(fish[0].1.ends_with(":/usr/local/share:/usr/share"));

        // Shells without an environment-only hook are untouched.
        assert!(env_pairs("bash", dir, None, None).is_empty());
        assert!(env_pairs("nu", dir, None, None).is_empty());
        assert!(env_pairs("", dir, None, None).is_empty());
    }

    #[test]
    fn materialize_writes_scripts_idempotently() {
        let base = std::env::temp_dir()
            .join(format!("rio-shell-integration-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);

        materialize(&base).unwrap();
        let zshenv = base.join("zsh").join(".zshenv");
        let fish = base
            .join("data")
            .join("fish")
            .join("vendor_conf.d")
            .join("rio.fish");
        assert_eq!(std::fs::read_to_string(&zshenv).unwrap(), ZSH_ZSHENV);
        assert!(base.join("zsh").join("rio-integration.zsh").exists());
        assert_eq!(std::fs::read_to_string(&fish).unwrap(), FISH_INTEGRATION);

        // A second run over existing content is a no-op, not an error.
        materialize(&base).unwrap();
        assert_eq!(std::fs::read_to_string(&zshenv).unwrap(), ZSH_ZSHENV);

        let _ = std::fs::remove_dir_all(&base);
    }
}
