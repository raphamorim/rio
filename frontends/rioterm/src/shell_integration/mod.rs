//! Automatic shell integration.
//!
//! Rio embeds small per-shell scripts that report the working
//! directory to the terminal (OSC 7, `kitty-shell-cwd://` flavor) on
//! every prompt and directory change; the zsh one also marks prompts
//! and commands with OSC 133 (`A`/`B` around the prompt, `C` before a
//! command runs, `D` with its status when it ends), which drives
//! `[notify-on-command-finish]`. fish 4 sends those marks itself. At
//! spawn time the scripts are
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
            .as_chunks::<2>()
            .0
            .iter()
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

    /// The materialized scripts plus a user ZDOTDIR holding only the
    /// given `.zshrc`, which keeps the developer's own zsh config out
    /// of a test. Removed on drop, so a failed assertion leaves
    /// nothing behind.
    #[cfg(unix)]
    struct ZshFixture {
        base: PathBuf,
        user_zdotdir: PathBuf,
    }

    #[cfg(unix)]
    impl ZshFixture {
        fn new(tag: &str, rc: &str) -> Self {
            let base = std::env::temp_dir()
                .join(format!("rio-si-zsh-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            materialize(&base).unwrap();
            let user_zdotdir = base.join("user-config");
            std::fs::create_dir_all(&user_zdotdir).unwrap();
            std::fs::write(user_zdotdir.join(".zshrc"), rc).unwrap();
            Self { base, user_zdotdir }
        }

        /// The pane's environment for zsh: the integration redirect
        /// with the user ZDOTDIR preserved.
        fn env(&self) -> Vec<(String, String)> {
            env_pairs(
                "zsh",
                &self.base,
                Some(self.user_zdotdir.to_string_lossy().into_owned()),
                None,
            )
        }
    }

    #[cfg(unix)]
    impl Drop for ZshFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    /// `zsh -i` without the system-wide rc files, whose hooks (macOS
    /// adds one per terminal program) would change what the test sees.
    #[cfg(unix)]
    const ZSH_ARGS: [&str; 3] = ["-i", "-o", "NO_GLOBAL_RCS"];

    /// Drives a real interactive zsh over a pipe against the
    /// materialized scripts, with `rc` as the user's `.zshrc`, and
    /// returns `(stdout, stderr)`. Commands arrive on the pipe, so zsh
    /// prompts without zle: the prompt goes to stderr and the hooks'
    /// marks to stdout. `None` where zsh is not installed; any other
    /// spawn failure is a test failure.
    #[cfg(unix)]
    fn run_zsh(tag: &str, rc: &str, input: &[u8]) -> Option<(String, String)> {
        use std::io::Write as _;
        use std::process::Stdio;

        let fixture = ZshFixture::new(tag, rc);
        let mut child = match std::process::Command::new("zsh")
            .args(ZSH_ARGS)
            .envs(fixture.env())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
            Err(e) => panic!("spawning zsh: {e}"),
        };
        let _ = child.stdin.take().unwrap().write_all(input);
        let output = child.wait_with_output().unwrap();
        Some((
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ))
    }

    /// Every OSC 133 mark in `text`, without its terminator.
    #[cfg(unix)]
    fn osc_133_marks(text: &str) -> Vec<&str> {
        text.match_indices("\x1b]133;")
            .map(|(start, _)| {
                let rest = &text[start..];
                // BEL or an ESC-started ST ends it; the leading ESC
                // is the mark's own.
                let end = rest[1..]
                    .find(['\x07', '\x1b'].as_slice())
                    .map_or(rest.len(), |end| end + 1);
                &rest[..end]
            })
            .collect()
    }

    /// The pane setup (ZDOTDIR redirect plus preserved user ZDOTDIR)
    /// must end with the integration loaded, the cwd reported, and
    /// every command bracketed by OSC 133 `C` and `D` with its status.
    /// Skips silently where zsh is not installed.
    #[cfg(unix)]
    #[test]
    fn zsh_integration_marks_prompts_and_commands() {
        // A success, a failure, an empty line, then exit.
        let Some((stdout, stderr)) = run_zsh("plain", "", b"true\nfalse\n\nexit\n")
        else {
            return;
        };

        assert!(
            stdout.contains("\x1b]7;kitty-shell-cwd://"),
            "no OSC 7 in zsh output; stdout: {stdout:?}, stderr: {stderr:?}"
        );

        // `C` before each command runs and `D` with its status before
        // the next prompt; the empty line closes with no status, and
        // `exit` leaves its `C` open.
        assert_eq!(
            osc_133_marks(&stdout),
            [
                "\x1b]133;C",
                "\x1b]133;D;0",
                "\x1b]133;C",
                "\x1b]133;D;1",
                "\x1b]133;D",
                "\x1b]133;C",
            ],
            "stdout: {stdout:?}, stderr: {stderr:?}"
        );

        // With no other hook, the prompt itself carries `A` and `B`
        // from the first one on, so a redraw repeats them.
        let prompts = stderr.matches("\x1b]133;A\x07").count();
        assert_eq!(prompts, 4, "stderr: {stderr:?}");
        assert_eq!(stderr.matches("\x1b]133;B\x07").count(), 4);
    }

    /// A prompt theme that sets a two-line PS1 from its own precmd
    /// hook, loaded from `.zshrc` after the integration: its PS1 is
    /// the one that gets marked, with the second line as a
    /// continuation, and both prompts are handed back clean to
    /// commands and hooks, never marked twice. The first prompt is
    /// marked with a bare `A`, since the theme sets PS1 after the
    /// integration's first run.
    #[cfg(unix)]
    #[test]
    fn zsh_integration_marks_a_theme_prompt_once() {
        let rc = "autoload -Uz add-zsh-hook\n\
                  theme_precmd() { PS1=$'L1\\nL2 %# ' }\n\
                  add-zsh-hook precmd theme_precmd\n";
        let input = b":\n:\n\nprint -r -- \"AT_CMD ${(q)PS1} ${(q)PS2}\"\nexit\n";
        let Some((stdout, stderr)) = run_zsh("theme", rc, input) else {
            return;
        };

        // The command sees the theme's own PS1 and the default PS2.
        assert!(
            stdout.contains("AT_CMD L1$'\\n'L2\\ %\\#\\  %_\\>\\ \n"),
            "prompts not clean at command time; stdout: {stdout:?}"
        );

        // Five prompts: the first with a bare `A` (no `B`), the other
        // four marked in PS1 with one `A`, one continuation and one
        // `B` each.
        assert_eq!(
            osc_133_marks(&stdout)
                .iter()
                .filter(|mark| mark.ends_with(";A"))
                .count(),
            1,
            "stdout: {stdout:?}"
        );
        let marked: Vec<&str> = osc_133_marks(&stderr);
        assert_eq!(
            marked,
            ["\x1b]133;A", "\x1b]133;P;k=s", "\x1b]133;B"].repeat(4),
            "stderr: {stderr:?}"
        );
    }

    /// A prompt ending in a lone `%` renders without it, marked or
    /// not; and the `C` mark comes before whatever other preexec
    /// hooks print, which see the clean prompt.
    #[cfg(unix)]
    #[test]
    fn zsh_integration_drops_a_lone_percent_and_marks_before_preexec_hooks() {
        let rc = "autoload -Uz add-zsh-hook\n\
                  PS1='x%'\n\
                  other_preexec() { print -r -- \"PRE:$PS1\" }\n\
                  add-zsh-hook preexec other_preexec\n";
        let Some((stdout, stderr)) = run_zsh("percent", rc, b":\nexit\n") else {
            return;
        };

        assert!(
            stderr.contains("\x07x\x1b]133;B\x07"),
            "lone percent kept or mark lost; stderr: {stderr:?}"
        );
        assert!(!stderr.contains("x%"), "stderr: {stderr:?}");

        let c = stdout.find("\x1b]133;C\x07").expect("no C mark");
        let pre = stdout.find("PRE:").expect("no preexec hook output");
        assert!(c < pre, "C after the hook's output; stdout: {stdout:?}");
        assert!(stdout.contains("PRE:x%\n"), "stdout: {stdout:?}");
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

    /// A hook that edits PS1 in place rather than setting it, the way
    /// a venv or conda prefix does. On a prompt with no command before
    /// it (Enter on an empty line) no preexec ran, so the hook sees
    /// the marked PS1 and wraps the marks; they must still come out,
    /// and the hook's prefix must stay, once.
    #[cfg(unix)]
    #[test]
    fn zsh_integration_unmarks_a_prompt_a_hook_edited() {
        let rc = "autoload -Uz add-zsh-hook\n\
                  venvish() { [[ $PS1 == '(v) '* ]] || PS1=\"(v) $PS1\" }\n\
                  add-zsh-hook precmd venvish\n";
        let Some((stdout, stderr)) = run_zsh("venv", rc, b":\n\n\n:\nexit\n") else {
            return;
        };

        // Five prompts: a bare `A` for the first, then one `A` and one
        // `B` in each of the other four, never doubled.
        assert_eq!(
            osc_133_marks(&stdout)
                .iter()
                .filter(|mark| mark.ends_with(";A"))
                .count(),
            1,
            "stdout: {stdout:?}"
        );
        assert_eq!(
            osc_133_marks(&stderr),
            ["\x1b]133;A", "\x1b]133;B"].repeat(4),
            "stderr: {stderr:?}"
        );
        assert_eq!(stderr.matches("(v) ").count(), 5, "stderr: {stderr:?}");
        assert!(!stderr.contains("(v) (v)"), "stderr: {stderr:?}");
    }

    /// The whole path, as a pane runs it: zsh on a real PTY (so zle is
    /// active and the prompt is drawn the way a user sees it) with the
    /// integration environment, its output fed through rio's own
    /// parser, which must report each command once with its status.
    /// Skips silently where zsh is not installed.
    #[cfg(unix)]
    #[test]
    fn zsh_integration_reports_finished_commands_through_the_parser() {
        use rio_backend::ansi::CursorShape;
        use rio_backend::crosswords::{Crosswords, CrosswordsSize};
        use rio_backend::event::{EventListener, RioEvent, WindowId};
        use rio_backend::performer::handler::Processor;
        use std::cell::RefCell;
        use std::io::{Read as _, Write as _};
        use std::rc::Rc;
        use std::time::{Duration, Instant};

        #[derive(Clone, Default)]
        struct Finished(Rc<RefCell<Vec<Option<i32>>>>);
        impl EventListener for Finished {
            fn send_event(&self, event: RioEvent, _: WindowId) {
                if let RioEvent::CommandFinished { exit_status, .. } = event {
                    self.0.borrow_mut().push(exit_status);
                }
            }
        }

        let fixture = ZshFixture::new("pty", "");
        let mut env = fixture.env();
        env.push(("TERM".to_string(), "xterm-256color".to_string()));

        let mut pty = match teletypewriter::create_pty_with_spawn(
            Some("zsh"),
            ZSH_ARGS.iter().map(|arg| arg.to_string()).collect(),
            &Some(fixture.base.to_string_lossy().into_owned()),
            Some(env),
            80,
            24,
            800,
            480,
        ) {
            Ok(pty) => pty,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => panic!("spawning zsh on a pty: {e}"),
        };

        let finished = Finished::default();
        let mut term = Crosswords::new(
            CrosswordsSize::new(80, 24),
            CursorShape::Block,
            finished.clone(),
            WindowId::from(0),
            0,
            100,
        );
        let mut parser = Processor::default();

        // Typed ahead: zle takes them one line at a time.
        pty.write_all(b"true\rfalse\rexit\r").unwrap();

        let deadline = Instant::now() + Duration::from_secs(20);
        let mut buf = [0u8; 4096];
        let mut transcript = Vec::new();
        while Instant::now() < deadline && finished.0.borrow().len() < 2 {
            match pty.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    transcript.extend_from_slice(&buf[..n]);
                    parser.advance(&mut term, &buf[..n]);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => break,
            }
        }

        drop(pty);
        assert_eq!(
            *finished.0.borrow(),
            [Some(0), Some(1)],
            "transcript: {:?}",
            String::from_utf8_lossy(&transcript)
        );
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
