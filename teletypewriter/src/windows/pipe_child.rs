//! Runs a program on plain pipes instead of a pseudoconsole.
//!
//! ConPTY redraws the program's output from a screen buffer, so sequences it
//! has no use for never arrive, tmux control mode among them. On pipes the
//! bytes arrive untouched, which suits programs whose real terminal is
//! elsewhere, like `ssh -tt`. Programs that need a console won't work: there
//! is no window size to report and nothing to read a password prompt from.

use std::fs::File;
use std::io::{Error, ErrorKind, Result};
use std::os::windows::io::{FromRawHandle, IntoRawHandle};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::System::Threading::{TerminateProcess, CREATE_NO_WINDOW};

use crate::windows::child::ChildExitWatcher;
use crate::windows::pipes::{EventedAnonRead, EventedAnonWrite};
use crate::windows::Pty;

/// Ends the child when the terminal goes away. Nothing else would: unlike a
/// pseudoconsole, closing a pipe doesn't take the program down with it.
pub struct PipeChild {
    process: HANDLE,
}

// The process handle can be sent between threads.
unsafe impl Send for PipeChild {}

impl Drop for PipeChild {
    fn drop(&mut self) {
        unsafe { TerminateProcess(self.process, 1) };
    }
}

pub fn new(
    program: Option<&str>,
    args: &[String],
    working_directory: &Option<String>,
    env: Option<Vec<(String, String)>>,
) -> Result<Pty> {
    let program = program
        .filter(|program| !program.is_empty())
        .ok_or_else(|| {
            Error::new(ErrorKind::InvalidInput, "use-pipes needs a shell program")
        })?;

    let (conout, child_stdout) = miow::pipe::anonymous(0)?;
    let (child_stdin, conin) = miow::pipe::anonymous(0)?;
    let child_stdin = unsafe { File::from_raw_handle(child_stdin.into_raw_handle()) };
    let child_stdout = unsafe { File::from_raw_handle(child_stdout.into_raw_handle()) };
    // A failed `ssh` only explains itself on stderr.
    let child_stderr = child_stdout.try_clone()?;

    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        // Rio has no console for the child to share, so without this Windows
        // opens a window for it.
        .creation_flags(CREATE_NO_WINDOW);
    if let Some(env) = env {
        command.envs(env);
    }
    if let Some(working_directory) = working_directory {
        command.current_dir(working_directory);
    }
    let child = command.spawn()?;
    // Holds our copies of the child's pipe ends, which would keep the output
    // pipe open after the child exits.
    drop(command);

    // As with ConPTY, the handle stays open for the exit watcher.
    let process = child.into_raw_handle() as HANDLE;
    let child_watcher = ChildExitWatcher::new(process)?;

    Ok(Pty::new(
        PipeChild { process },
        EventedAnonRead::new(conout),
        EventedAnonWrite::new(conin),
        child_watcher,
    ))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::{ChildEvent, EventedPty, ProcessReadWrite};

    /// Reads until `done` accepts what has arrived.
    fn read_until(pty: &mut Pty, done: impl Fn(&[u8]) -> bool) -> Vec<u8> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut output = Vec::new();
        let mut buffer = [0; 1024];
        while !done(&output) {
            assert!(Instant::now() < deadline, "only got {output:?}");
            match pty.reader().read(&mut buffer) {
                // Nothing has arrived yet, or the pipe has closed.
                Ok(0) => std::thread::sleep(Duration::from_millis(10)),
                Ok(count) => output.extend_from_slice(&buffer[..count]),
                Err(error) => panic!("{error}"),
            }
        }
        output
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|part| part == needle)
    }

    fn cmd(script: &str) -> Pty {
        let args = ["/c".to_string(), script.to_string()];
        new(Some("cmd.exe"), &args, &None, None).unwrap()
    }

    #[test]
    fn dcs_survives() {
        // What ConPTY strips, with bytes above 0x7f for good measure.
        let bytes = b"BEFORE\x1bP1000p%begin\xe2\x9c\x9c\x1b\\AFTER";
        let directory = std::env::temp_dir();
        let name = format!("rio-pipe-child-{}.bin", std::process::id());
        std::fs::write(directory.join(&name), bytes).unwrap();

        let args = ["/c".to_string(), format!("type {name}")];
        let working_directory = Some(directory.display().to_string());
        let mut pty = new(Some("cmd.exe"), &args, &working_directory, None).unwrap();
        let output = read_until(&mut pty, |output| contains(output, b"AFTER"));
        std::fs::remove_file(directory.join(&name)).unwrap();
        assert!(contains(&output, bytes), "got {output:?}");
    }

    #[test]
    fn stdin_reaches_the_child() {
        let args = [
            "/v:on".to_string(),
            "/c".to_string(),
            "set /p x=&echo got !x!".to_string(),
        ];
        let mut pty = new(Some("cmd.exe"), &args, &None, None).unwrap();
        pty.writer().write_all(b"hello\r\n").unwrap();
        pty.writer().flush().unwrap();
        read_until(&mut pty, |output| contains(output, b"got hello"));
    }

    #[test]
    fn stderr_is_shown() {
        let mut pty = cmd("echo oops 1>&2");
        read_until(&mut pty, |output| contains(output, b"oops"));
    }

    #[test]
    fn env_and_working_directory_apply() {
        let directory = std::env::temp_dir();
        let args = ["/c".to_string(), "echo %RIO_PIPE_TEST% & cd".to_string()];
        let env = vec![("RIO_PIPE_TEST".to_string(), "from-rio".to_string())];
        let working_directory = Some(directory.display().to_string());
        let mut pty = new(Some("cmd.exe"), &args, &working_directory, Some(env)).unwrap();

        let name = directory
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let output = read_until(&mut pty, |output| {
            contains(output, b"from-rio") && contains(output, name.as_bytes())
        });
        assert!(contains(&output, b"from-rio"));
    }

    #[test]
    fn exit_is_reported() {
        let mut pty = cmd("exit 7");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match pty.next_child_event() {
                Some(event) => {
                    assert_eq!(event, ChildEvent::Exited(Some(7)));
                    break;
                }
                None => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }

    #[test]
    fn writing_after_exit_fails_cleanly() {
        let mut pty = cmd("exit");
        let deadline = Instant::now() + Duration::from_secs(10);
        while pty.next_child_event().is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        }

        // The worker only notices on a write, and reports it on the next.
        let failed = (0..500).any(|_| {
            std::thread::sleep(Duration::from_millis(10));
            pty.writer().write_all(b"x").is_err()
        });
        assert!(failed);
        // Dropping must cope with the worker having gone.
    }

    #[test]
    fn resize_is_ignored() {
        let mut pty = cmd("exit");
        pty.set_winsize(crate::WinsizeBuilder {
            rows: 10,
            cols: 10,
            width: 0,
            height: 0,
        })
        .unwrap();
    }

    #[test]
    fn dropping_ends_the_child() {
        let pty = cmd("ping -n 60 127.0.0.1");
        let process = pty.child_watcher().raw_handle();
        drop(pty);

        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        assert_eq!(
            unsafe { WaitForSingleObject(process, 10_000) },
            WAIT_OBJECT_0
        );
    }

    #[test]
    fn a_program_is_required() {
        assert_eq!(
            new(None, &[], &None, None).err().map(|error| error.kind()),
            Some(ErrorKind::InvalidInput)
        );
    }
}
