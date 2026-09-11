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
//! `vendor_conf.d` loads from there (the script restores the original
//! value so the prepend never leaks to child processes). PowerShell
//! has no environment hook, so a bare spawn (no configured args) is
//! rewritten to `-NoExit -EncodedCommand <script>` carrying the script
//! inline, which runs after the user's profile and is exempt from
//! execution policy (the default Windows client policy blocks script
//! FILES, so a dot-sourced file would error in every pane). Shells
//! with neither hook (bash needs `--posix` argv surgery, cmd.exe only
//! has a machine-wide registry key) are left untouched, and every
//! integrated pane exports `RIO_SHELL_INTEGRATION` pointing at the
//! script directory so any shell can source the integration manually.
//!
//! Old versions' script directories are deliberately left in place: a
//! concurrently running older rio still spawns shells whose `ZDOTDIR`
//! points into its own directory, and deleting it would make zsh skip
//! the user's entire configuration.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const ZSH_ZSHENV: &str = include_str!("zshenv.zsh");
const ZSH_INTEGRATION: &str = include_str!("rio-integration.zsh");
const FISH_INTEGRATION: &str = include_str!("rio.fish");
const POWERSHELL_INTEGRATION: &str = include_str!("rio.ps1");

/// Everything shell integration wants applied to one spawn: extra
/// environment, and for PowerShell a replacement `(program, args)`
/// command (the program stays `None` when the shell was unconfigured,
/// so platform default-shell handling such as macOS `login(1)` still
/// wraps the spawn).
#[derive(Default)]
pub struct SpawnIntegration {
    pub env: Vec<(String, String)>,
    pub command: Option<(Option<String>, Vec<String>)>,
}

/// Resolve the shell once and derive both integration halves from it.
/// Empty when the scripts could not be written.
pub fn prepare(shell_program: Option<&str>, args: &[String]) -> SpawnIntegration {
    let program = resolved_shell(shell_program);
    let shell_name = shell_display_name(&program);
    let Some(dir) = integration_dir() else {
        return SpawnIntegration::default();
    };
    let mut env = env_pairs(
        &shell_name,
        dir,
        std::env::var("ZDOTDIR").ok(),
        std::env::var("XDG_DATA_DIRS").ok(),
    );
    env.push((
        "RIO_SHELL_INTEGRATION".to_string(),
        dir.to_string_lossy().to_string(),
    ));
    let command = powershell_command(&shell_name, shell_program, &program, args);
    SpawnIntegration { env, command }
}

/// The program the PTY will spawn: the configured one, else the same
/// resolution the spawn itself performs (`$SHELL`, then the passwd
/// entry, on unix; `powershell` on Windows).
fn resolved_shell(shell_program: Option<&str>) -> String {
    match shell_program {
        Some(program) if !program.is_empty() => program.to_string(),
        #[cfg(not(target_os = "windows"))]
        _ => teletypewriter::default_shell_program(),
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
            // The original value rides along (empty means "was unset")
            // so the script can RESTORE it after loading: without the
            // restore, every process the pane ever starts inherits the
            // rio-version-specific prepend.
            let (restore, dirs) = match current_xdg_data_dirs {
                Some(existing) if !existing.is_empty() => {
                    let dirs = format!("{data_dir}:{existing}");
                    (existing, dirs)
                }
                // The XDG spec default applies when the variable is
                // unset; spell it out so prepending does not hide it.
                _ => (
                    String::new(),
                    format!("{data_dir}:/usr/local/share:/usr/share"),
                ),
            };
            vec![
                ("RIO_FISH_XDG_DATA_DIRS".to_string(), restore),
                ("XDG_DATA_DIRS".to_string(), dirs),
            ]
        }
        _ => Vec::new(),
    }
}

/// Rewrites a PowerShell spawn (powershell.exe or pwsh, any platform)
/// so the shell loads rio's integration script AFTER the user's
/// profile ran. Only a bare spawn is rewritten: configured args change
/// what the command line means, so they win over integration. The
/// returned program keeps an unconfigured shell unconfigured on unix,
/// so the platform's default-shell handling (macOS `login(1)`) still
/// wraps the spawn and only the args ride through it; Windows names
/// the platform default explicitly because its PTY drops args when no
/// program is given.
fn powershell_command(
    shell_name: &str,
    configured_program: Option<&str>,
    resolved_program: &str,
    args: &[String],
) -> Option<(Option<String>, Vec<String>)> {
    if shell_name != "powershell" && shell_name != "pwsh" {
        return None;
    }
    if !args.is_empty() {
        tracing::info!("shell integration skipped: PowerShell spawn has configured args");
        return None;
    }
    #[cfg(target_os = "windows")]
    let program = Some(resolved_program.to_string());
    #[cfg(not(target_os = "windows"))]
    let program = {
        let _ = resolved_program;
        configured_program.map(str::to_string)
    };
    Some((program, powershell_args()))
}

/// `-NoExit -EncodedCommand <base64 of the UTF-16LE script>`: the
/// script travels inline, so no file is dot-sourced (script FILES are
/// what the default Windows execution policy blocks) and no quoting
/// can break. Encoded per call: this runs once per PowerShell pane,
/// where microseconds of base64 vanish next to the process spawn.
fn powershell_args() -> Vec<String> {
    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    let utf16le: Vec<u8> = POWERSHELL_INTEGRATION
        .encode_utf16()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    vec![
        "-NoExit".to_string(),
        "-EncodedCommand".to_string(),
        B64.encode(utf16le),
    ]
}

/// The on-disk script directory, materialized once per process. Keyed
/// by rio's version so upgrades write fresh scripts and downgrades
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
        // The join separator differs per platform; compare against the
        // same join the implementation performs.
        let zsh_dir = dir.join("zsh").to_string_lossy().to_string();
        let data_dir = dir.join("data").to_string_lossy().to_string();

        let zsh = env_pairs("zsh", dir, None, None);
        assert_eq!(zsh, vec![("ZDOTDIR".to_string(), zsh_dir.clone())]);

        // A user ZDOTDIR is preserved for the .zshenv chain to restore.
        let zsh = env_pairs("zsh", dir, Some("/home/u/.config/zsh".into()), None);
        assert_eq!(
            zsh[0],
            (
                "RIO_ZSH_ZDOTDIR".to_string(),
                "/home/u/.config/zsh".to_string()
            )
        );
        assert_eq!(zsh[1], ("ZDOTDIR".to_string(), zsh_dir));

        // A user XDG_DATA_DIRS is preserved for rio.fish to restore.
        let fish = env_pairs("fish", dir, None, Some("/usr/share".into()));
        assert_eq!(
            fish,
            vec![
                (
                    "RIO_FISH_XDG_DATA_DIRS".to_string(),
                    "/usr/share".to_string()
                ),
                (
                    "XDG_DATA_DIRS".to_string(),
                    format!("{data_dir}:/usr/share")
                ),
            ]
        );

        // Unset XDG_DATA_DIRS keeps the spec default visible, and the
        // empty restore value tells the script to unset it again.
        let fish = env_pairs("fish", dir, None, None);
        assert_eq!(
            fish[0],
            ("RIO_FISH_XDG_DATA_DIRS".to_string(), String::new())
        );
        assert!(fish[1].1.ends_with(":/usr/local/share:/usr/share"));

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
    fn powershell_invocation_carries_the_script_inline() {
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine as _;

        let args = powershell_args();
        assert_eq!(args[0], "-NoExit");
        assert_eq!(args[1], "-EncodedCommand");

        // The payload decodes back to the exact script, UTF-16LE.
        let bytes = B64.decode(&args[2]).unwrap();
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        assert_eq!(String::from_utf16(&units).unwrap(), POWERSHELL_INTEGRATION);
    }

    #[test]
    fn powershell_command_scope() {
        // Configured args always win over integration.
        assert!(
            powershell_command("pwsh", Some("pwsh"), "pwsh", &["-NoLogo".into()])
                .is_none()
        );
        // Non-PowerShell shells are untouched.
        assert!(powershell_command("zsh", Some("zsh"), "zsh", &[]).is_none());

        let (program, args) =
            powershell_command("pwsh", Some("pwsh"), "pwsh", &[]).unwrap();
        assert_eq!(program.as_deref(), Some("pwsh"));
        assert_eq!(args[1], "-EncodedCommand");
    }

    /// Runs a real zsh against the materialized scripts: the pane
    /// setup (ZDOTDIR redirect plus preserved user ZDOTDIR) must end
    /// with the integration loaded and one cwd report emitted. Skips
    /// silently where zsh is not installed.
    #[cfg(unix)]
    #[test]
    fn zsh_integration_emits_a_cwd_report() {
        let base =
            std::env::temp_dir().join(format!("rio-si-zsh-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        materialize(&base).unwrap();
        // An empty restored ZDOTDIR keeps the developer's own zsh
        // config out of the test.
        let user_zdotdir = base.join("empty-user-config");
        std::fs::create_dir_all(&user_zdotdir).unwrap();

        let Ok(output) = std::process::Command::new("zsh")
            .args(["-ic", ":"])
            .env("ZDOTDIR", base.join("zsh"))
            .env("RIO_ZSH_ZDOTDIR", &user_zdotdir)
            .output()
        else {
            return;
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("\x1b]7;kitty-shell-cwd://"),
            "no OSC 7 in zsh output; stdout: {stdout:?}, stderr: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// Same, for fish, via its `vendor_conf.d` loading from
    /// `XDG_DATA_DIRS`, which the script must then RESTORE so the
    /// prepend never leaks to child processes. Skips silently where
    /// fish is not installed (no CI runner ships it today, so this
    /// mainly guards local changes to the fish script).
    #[cfg(unix)]
    #[test]
    fn fish_integration_emits_a_cwd_report_and_restores_env() {
        let base =
            std::env::temp_dir().join(format!("rio-si-fish-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        materialize(&base).unwrap();
        // An empty config home keeps the developer's own fish config
        // out of the test.
        let config_home = base.join("empty-config-home");
        std::fs::create_dir_all(&config_home).unwrap();

        let Ok(output) = std::process::Command::new("fish")
            .args(["-ic", "echo RIO_XDG=$XDG_DATA_DIRS"])
            .env("XDG_DATA_DIRS", base.join("data"))
            .env("RIO_FISH_XDG_DATA_DIRS", "/usr/share")
            .env("XDG_CONFIG_HOME", &config_home)
            .output()
        else {
            return;
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.contains("\x1b]7;kitty-shell-cwd://"),
            "no OSC 7 in fish output; stdout: {stdout:?}, stderr: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            stdout.contains("RIO_XDG=/usr/share"),
            "XDG_DATA_DIRS not restored; stdout: {stdout:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
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
        assert!(base.join("powershell").join("rio.ps1").exists());

        // A second run over existing content is a no-op, not an error.
        materialize(&base).unwrap();
        assert_eq!(std::fs::read_to_string(&zshenv).unwrap(), ZSH_ZSHENV);

        let _ = std::fs::remove_dir_all(&base);
    }
}
