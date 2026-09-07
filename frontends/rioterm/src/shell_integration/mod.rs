//! Automatic shell integration.
//!
//! Rio embeds small per-shell scripts that report the working
//! directory to the terminal (OSC 7, `kitty-shell-cwd://` flavor) on
//! every prompt and directory change. At spawn time the scripts are
//! written under the user's cache directory (once per rio version)
//! and the child is pointed at them. zsh and fish load through the
//! environment alone: `ZDOTDIR` for zsh (the user's value is preserved
//! in `RIO_ZSH_ZDOTDIR` and restored before any of their configuration
//! runs) and an `XDG_DATA_DIRS` prepend for fish, whose
//! `vendor_conf.d` loads from there. PowerShell has no environment
//! hook, so a bare spawn (no configured args) is rewritten to
//! `-NoExit -Command . '<script>'`, which runs after the user's
//! profile. Shells with neither hook (bash needs `--posix` argv
//! surgery, cmd.exe only has a machine-wide registry key) are left
//! untouched.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const ZSH_ZSHENV: &str = include_str!("zshenv.zsh");
const ZSH_INTEGRATION: &str = include_str!("rio-integration.zsh");
const FISH_INTEGRATION: &str = include_str!("rio.fish");
const POWERSHELL_INTEGRATION: &str = include_str!("rio.ps1");

/// The program the PTY will spawn: the configured one, else the same
/// platform default the spawn itself falls back to.
fn resolved_shell(shell_program: Option<&str>) -> String {
    match shell_program {
        Some(program) if !program.is_empty() => program.to_string(),
        #[cfg(not(target_os = "windows"))]
        _ => std::env::var("SHELL").unwrap_or_default(),
        #[cfg(target_os = "windows")]
        _ => String::from("powershell"),
    }
}

/// The shell's identity from its program path: basename with both
/// separator flavors (a Windows path can carry `\`), lowercased,
/// `.exe` dropped, so `C:\W\pwsh.exe` and `/usr/bin/pwsh` compare
/// equal.
fn shell_display_name(program: &str) -> String {
    let name = program.rsplit(['/', '\\']).next().unwrap_or(program);
    let name = name.to_ascii_lowercase();
    name.strip_suffix(".exe").unwrap_or(&name).to_string()
}

/// Extra environment for the pane about to spawn `shell_program`.
/// Empty when the shell has no environment-only integration hook or
/// the scripts could not be written.
pub fn spawn_env(shell_program: Option<&str>) -> Vec<(String, String)> {
    let shell_name = shell_display_name(&resolved_shell(shell_program));
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

/// Rewrites a PowerShell spawn (powershell.exe or pwsh, any platform)
/// so the shell loads rio's integration script AFTER the user's
/// profile ran. Only a bare spawn is rewritten: configured args change
/// what `-Command` would mean, so they win over integration.
pub fn powershell_command(
    shell_program: Option<&str>,
    args: &[String],
) -> Option<(String, Vec<String>)> {
    if !args.is_empty() {
        return None;
    }
    let program = resolved_shell(shell_program);
    let name = shell_display_name(&program);
    if name != "powershell" && name != "pwsh" {
        return None;
    }
    let script = integration_dir()?.join("powershell").join("rio.ps1");
    Some((program, powershell_args(&script)))
}

/// `-NoExit -Command . '<script>'`, with the path single-quoted so
/// spaces survive and embedded quotes doubled per PowerShell quoting.
fn powershell_args(script: &Path) -> Vec<String> {
    let script = script.to_string_lossy().replace('\'', "''");
    vec![
        "-NoExit".to_string(),
        "-Command".to_string(),
        format!(". '{script}'"),
    ]
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

    let powershell = base.join("powershell");
    std::fs::create_dir_all(&powershell)?;
    write_if_changed(&powershell.join("rio.ps1"), POWERSHELL_INTEGRATION)?;
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
        assert!(env_pairs("powershell", dir, None, None).is_empty());
        assert!(env_pairs("nu", dir, None, None).is_empty());
        assert!(env_pairs("", dir, None, None).is_empty());
    }

    #[test]
    fn shell_names_normalize_across_platforms() {
        assert_eq!(shell_display_name("/usr/local/bin/fish"), "fish");
        assert_eq!(shell_display_name("zsh"), "zsh");
        assert_eq!(
            shell_display_name("C:\\Program Files\\PowerShell\\7\\pwsh.exe"),
            "pwsh"
        );
        assert_eq!(
            shell_display_name(
                "C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe"
            ),
            "powershell"
        );
        assert_eq!(shell_display_name("PWSH.EXE"), "pwsh");
    }

    #[test]
    fn powershell_invocation_quotes_the_script_path() {
        let args = powershell_args(Path::new("/tmp/o'brien/rio.ps1"));
        assert_eq!(args[0], "-NoExit");
        assert_eq!(args[1], "-Command");
        assert_eq!(args[2], ". '/tmp/o''brien/rio.ps1'");
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
