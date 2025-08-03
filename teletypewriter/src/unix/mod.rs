#![cfg(unix)]

#[cfg(target_os = "macos")]
mod macos;
mod signals;

extern crate libc;

use crate::{ChildEvent, EventedPty, ProcessReadWrite, Winsize, WinsizeBuilder};
use corcovado::unix::EventedFd;
#[cfg(target_os = "macos")]
use macos::*;
use signal_hook::consts as sigconsts;
use signals::Signals;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::io::Error;
use std::mem::MaybeUninit;
use std::ops::Deref;
use std::os::fd::OwnedFd;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr;
use std::sync::Arc;

#[cfg(all(target_os = "linux", not(target_env = "musl")))]
const TIOCSWINSZ: libc::c_ulong = 0x5414;
#[cfg(all(target_os = "linux", target_env = "musl"))]
const TIOCSWINSZ: libc::c_int = 0x5414;
#[cfg(target_os = "freebsd")]
const TIOCSWINSZ: libc::c_ulong = 0x80087467;
#[cfg(target_os = "macos")]
const TIOCSWINSZ: libc::c_ulong = 2148037735;

#[link(name = "util")]
extern "C" {
    fn forkpty(
        main: *mut libc::c_int,
        name: *mut libc::c_char,
        termp: *const libc::termios,
        winsize: *const Winsize,
    ) -> libc::pid_t;

    fn openpty(
        main: *mut libc::c_int,
        child: *mut libc::c_int,
        name: *mut libc::c_char,
        termp: *const libc::termios,
        winsize: *const Winsize,
    ) -> libc::pid_t;

    fn waitpid(
        pid: libc::pid_t,
        status: *mut libc::c_int,
        options: libc::c_int,
    ) -> libc::pid_t;

    fn ptsname(fd: *mut libc::c_int) -> *mut libc::c_char;
}

#[cfg(target_os = "macos")]
fn default_shell_command(shell: &str) {
    let command_shell_string = CString::new(shell).unwrap();
    let command_pointer = command_shell_string.as_ptr();
    let args = CString::new("--login").unwrap();
    let args_pointer = args.as_ptr();
    unsafe {
        libc::execvp(command_pointer, vec![args_pointer].as_ptr());
    }
}

#[cfg(not(target_os = "macos"))]
fn default_shell_command(shell: &str) {
    let command_shell_string = CString::new(shell).unwrap();
    let command_pointer = command_shell_string.as_ptr();
    // let home = std::env::var("HOME").unwrap();
    // let args = CString::new(home).unwrap();
    // let args_pointer = args.as_ptr() as *const i8;
    unsafe {
        libc::execvp(
            command_pointer,
            vec![command_pointer, std::ptr::null()].as_ptr(),
        );
    }
}

pub struct Pty {
    pub child: Child,
    file: File,
    token: corcovado::Token,
    signals_token: corcovado::Token,
    signals: Signals,
}

impl Deref for Pty {
    type Target = Child;
    fn deref(&self) -> &Child {
        &self.child
    }
}

impl io::Write for Pty {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match unsafe {
            libc::write(
                *self.child,
                buf.as_ptr() as *const _,
                buf.len() as libc::size_t,
            )
        } {
            n if n >= 0 => Ok(n as usize),
            _ => Err(io::Error::last_os_error()),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Pty {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match unsafe {
            libc::read(
                *self.child,
                buf.as_mut_ptr() as *mut _,
                buf.len() as libc::size_t,
            )
        } {
            n if n >= 0 => Ok(n as usize),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl ProcessReadWrite for Pty {
    type Reader = File;
    type Writer = File;

    #[inline]
    fn reader(&mut self) -> &mut File {
        &mut self.file
    }

    #[inline]
    fn read_token(&self) -> corcovado::Token {
        self.token
    }

    #[inline]
    fn writer(&mut self) -> &mut File {
        &mut self.file
    }

    #[inline]
    fn write_token(&self) -> corcovado::Token {
        self.token
    }

    #[inline]
    fn set_winsize(&mut self, winsize: WinsizeBuilder) -> Result<(), std::io::Error> {
        self.child.set_winsize(winsize)
    }

    #[inline]
    fn register(
        &mut self,
        poll: &corcovado::Poll,
        token: &mut dyn Iterator<Item = corcovado::Token>,
        interest: corcovado::Ready,
        poll_opts: corcovado::PollOpt,
    ) -> io::Result<()> {
        self.token = token.next().unwrap();
        poll.register(
            &EventedFd(&self.file.as_raw_fd()),
            self.token,
            interest,
            poll_opts,
        )?;

        self.signals_token = token.next().unwrap();
        poll.register(
            &self.signals,
            self.signals_token,
            corcovado::Ready::readable(),
            corcovado::PollOpt::level(),
        )
    }

    fn reregister(
        &mut self,
        poll: &corcovado::Poll,
        interest: corcovado::Ready,
        poll_opts: corcovado::PollOpt,
    ) -> io::Result<()> {
        poll.reregister(
            &EventedFd(&self.file.as_raw_fd()),
            self.token,
            interest,
            poll_opts,
        )?;

        poll.reregister(
            &self.signals,
            self.signals_token,
            corcovado::Ready::readable(),
            corcovado::PollOpt::level(),
        )
    }

    fn deregister(&mut self, poll: &corcovado::Poll) -> io::Result<()> {
        poll.deregister(&EventedFd(&self.file.as_raw_fd()))?;
        poll.deregister(&self.signals)
    }
}

// From alacritty: https://github.com/alacritty/alacritty/blob/2df8f860b960d7c96efaf4f059fe2fbbdce82bcc/alacritty_terminal/src/tty/mod.rs#L83
/// Check if a terminfo entry exists on the system.
pub fn terminfo_exists(terminfo: &str) -> bool {
    // Get first terminfo character for the parent directory.
    let first = terminfo.get(..1).unwrap_or_default();
    let first_hex = format!("{:x}", first.chars().next().unwrap_or_default() as usize);

    // Return true if the terminfo file exists at the specified location.
    macro_rules! check_path {
        ($path:expr) => {
            if $path.join(first).join(terminfo).exists()
                || $path.join(&first_hex).join(terminfo).exists()
            {
                return true;
            }
        };
    }

    if let Some(dir) = std::env::var_os("TERMINFO") {
        check_path!(PathBuf::from(&dir));
    } else if let Some(home) = dirs::home_dir() {
        check_path!(home.join(".terminfo"));
    }

    if let Ok(dirs) = std::env::var("TERMINFO_DIRS") {
        for dir in dirs.split(':') {
            check_path!(PathBuf::from(dir));
        }
    }

    if let Ok(prefix) = std::env::var("PREFIX") {
        let path = PathBuf::from(prefix);
        check_path!(path.join("etc/terminfo"));
        check_path!(path.join("lib/terminfo"));
        check_path!(path.join("share/terminfo"));
    }

    check_path!(PathBuf::from("/etc/terminfo"));
    check_path!(PathBuf::from("/lib/terminfo"));
    check_path!(PathBuf::from("/usr/share/terminfo"));
    check_path!(PathBuf::from("/boot/system/data/terminfo"));

    // No valid terminfo path has been found.
    false
}

pub fn create_termp(utf8: bool) -> libc::termios {
    // musl libc does not provide c_ispeed and c_ospeed fields in struct termios.
    #[cfg(target_os = "linux")]
    let mut term = libc::termios {
        c_iflag: libc::ICRNL | libc::IXON | libc::IXANY | libc::IMAXBEL | libc::BRKINT,
        c_oflag: libc::OPOST | libc::ONLCR,
        c_cflag: libc::CREAD | libc::CS8 | libc::HUPCL,
        c_lflag: libc::ICANON
            | libc::ISIG
            | libc::IEXTEN
            | libc::ECHO
            | libc::ECHOE
            | libc::ECHOK
            | libc::ECHOKE
            | libc::ECHOCTL,
        c_cc: Default::default(),
        #[cfg(not(target_env = "musl"))]
        c_ispeed: Default::default(),
        #[cfg(not(target_env = "musl"))]
        c_ospeed: Default::default(),
        #[cfg(target_env = "musl")]
        __c_ispeed: Default::default(),
        #[cfg(target_env = "musl")]
        __c_ospeed: Default::default(),
        c_line: 0,
    };

    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    let mut term = libc::termios {
        c_iflag: libc::ICRNL | libc::IXON | libc::IXANY | libc::IMAXBEL | libc::BRKINT,
        c_oflag: libc::OPOST | libc::ONLCR,
        c_cflag: libc::CREAD | libc::CS8 | libc::HUPCL,
        c_lflag: libc::ICANON
            | libc::ISIG
            | libc::IEXTEN
            | libc::ECHO
            | libc::ECHOE
            | libc::ECHOK
            | libc::ECHOKE
            | libc::ECHOCTL,
        c_cc: Default::default(),
        c_ispeed: Default::default(),
        c_ospeed: Default::default(),
    };

    #[cfg(not(target_os = "freebsd"))]
    {
        // Enable utf8 support if requested
        if utf8 {
            term.c_iflag |= libc::IUTF8;
        }
    }

    // Set supported terminal characters
    term.c_cc[libc::VEOF] = 4;
    term.c_cc[libc::VEOL] = 255;
    term.c_cc[libc::VEOL2] = 255;
    term.c_cc[libc::VERASE] = 0x7f;
    term.c_cc[libc::VWERASE] = 23;
    term.c_cc[libc::VKILL] = 21;
    term.c_cc[libc::VREPRINT] = 18;
    term.c_cc[libc::VINTR] = 3;
    term.c_cc[libc::VQUIT] = 0x1c;
    term.c_cc[libc::VSUSP] = 26;
    term.c_cc[libc::VSTART] = 17;
    term.c_cc[libc::VSTOP] = 19;
    term.c_cc[libc::VLNEXT] = 22;
    term.c_cc[libc::VDISCARD] = 15;
    term.c_cc[libc::VMIN] = 1;
    term.c_cc[libc::VTIME] = 0;

    #[cfg(target_os = "macos")]
    {
        term.c_cc[libc::VDSUSP] = 25;
        term.c_cc[libc::VSTATUS] = 20;
    }

    term
}

#[derive(Default)]
struct ShellUser {
    user: String,
    home: String,
    shell: String,
}

impl ShellUser {
    /// look for shell, username, longname, and home dir in the respective environment variables
    /// before falling back on looking in to `passwd`.
    fn from_env() -> Result<Self, Error> {
        let mut buf = [0; 1024];
        let pw = get_pw_entry(&mut buf);

        let user = match std::env::var("USER") {
            Ok(user) => user,
            Err(_) => match pw {
                Ok(ref pw) => pw.name.to_owned(),
                Err(err) => return Err(err),
            },
        };

        let home = match std::env::var("HOME") {
            Ok(home) => home,
            Err(_) => match pw {
                Ok(ref pw) => pw.dir.to_owned(),
                Err(err) => return Err(err),
            },
        };

        #[allow(unused_mut)]
        let mut shell = match std::env::var("SHELL") {
            Ok(env_shell) => env_shell,
            Err(_) => match pw {
                Ok(ref pw) => pw.shell.to_owned(),
                Err(err) => return Err(err),
            },
        };

        Ok(Self { user, home, shell })
    }
}

///
/// Creates a pseudoterminal using spawn.
///
/// The [`create_pty`] creates a pseudoterminal with similar behavior as tty,
/// which is a command in Unix and Unix-like operating systems to print the file name of the
/// terminal connected to standard input. tty stands for TeleTYpewriter.
///
/// It returns two [`Pty`] along with respective process name [`String`] and process id (`libc::pid_`)
///
pub fn create_pty_with_spawn(
    shell: &str,
    args: Vec<String>,
    working_directory: &Option<String>,
    columns: u16,
    rows: u16,
) -> Result<Pty, Error> {
    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    let mut is_controling_terminal = true;

    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    let is_controling_terminal = true;

    let mut main: libc::c_int = 0;
    let mut child: libc::c_int = 0;
    let winsize = Winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: columns as libc::c_ushort,
        ws_width: 0 as libc::c_ushort,
        ws_height: 0 as libc::c_ushort,
    };
    let term = create_termp(true);

    let res = unsafe {
        openpty(
            &mut main as *mut _,
            &mut child as *mut _,
            ptr::null_mut(),
            &term as *const libc::termios,
            &winsize as *const _,
        )
    };

    if res < 0 {
        return Err(Error::other("openpty failed"));
    }

    let mut shell_program = shell;

    let user = match ShellUser::from_env() {
        Ok(data) => data,
        Err(..) => ShellUser {
            shell: shell.to_string(),
            ..Default::default()
        },
    };

    if shell.is_empty() {
        shell_program = &user.shell;
    }

    tracing::info!("spawn {:?} {:?}", shell_program, args);

    let mut builder = {
        #[cfg(target_os = "macos")]
        {
            // On macOS, use /usr/bin/login to ensure proper login shell environment
            // This ensures PATH includes directories like /usr/local/bin
            let shell_name = shell_program.rsplit('/').next().unwrap_or(shell_program);
            let mut login_cmd = Command::new("/usr/bin/login");

            // Check for .hushlogin in home directory
            let hushlogin_path = std::path::Path::new(&user.home).join(".hushlogin");
            let flags = if hushlogin_path.exists() {
                "-qflp"
            } else {
                "-flp"
            };

            // -f: Bypasses authentication for already-logged-in user
            // -l: Skips changing directory to $HOME
            // -p: Preserves environment
            // -q: Act as if .hushlogin exists
            login_cmd.args([flags, &user.user]);

            // Build the exec command to replace the intermediate shell with our target shell
            let exec_cmd = if args.is_empty() {
                format!("exec -a -{shell_name} {shell_program}")
            } else {
                format!(
                    "exec -a -{} {} {}",
                    shell_name,
                    shell_program,
                    args.join(" ")
                )
            };

            // Use /bin/zsh as intermediate shell because it supports 'exec -a'
            login_cmd.args(["/bin/zsh", "-fc", &exec_cmd]);

            login_cmd
        }

        #[cfg(not(target_os = "macos"))]
        {
            let mut cmd = Command::new(shell_program);
            cmd.args(args);
            cmd
        }
    };

    #[cfg(target_os = "linux")]
    {
        // If running inside a flatpak sandbox.
        // Must retrieve $SHELL from outside the sandbox, so ask the host.
        if std::path::PathBuf::from("/.flatpak-info").exists() {
            builder = Command::new("flatpak-spawn");

            let mut with_args = vec![
                "--host".to_string(),
                "--watch-bus".to_string(),
                "--env=COLORTERM=truecolor".to_string(),
                "--env=TERM=rio".to_string(),
            ];

            if let Some(directory) = working_directory {
                with_args.push(format!(
                    "--directory={}",
                    std::path::Path::new(directory).display()
                ));
            }

            let output = std::process::Command::new("flatpak-spawn")
                .args(["--host", "sh", "-c", "echo $SHELL"])
                .output()?;
            let shell = String::from_utf8_lossy(&output.stdout);

            with_args.push(shell.trim().to_string());
            with_args.push("-l".to_string());

            builder.args(with_args);

            is_controling_terminal = false;
        }
    }

    // Setup child stdin/stdout/stderr as child fd of PTY.
    // Ownership of fd is transferred to the Stdio structs and will be closed by them at the end of
    // this scope. (It is not an issue that the fd is closed three times since File::drop ignores
    // error on libc::close.).
    let owned_child = unsafe { OwnedFd::from_raw_fd(child) };

    builder.stdin(owned_child.try_clone()?);
    builder.stderr(owned_child.try_clone()?);
    builder.stdout(owned_child);

    builder.env("USER", user.user);
    builder.env("HOME", user.home);

    unsafe {
        builder.pre_exec(move || {
            // Create a new process group.
            let err = libc::setsid();
            if err == -1 {
                return Err(Error::other("Failed to set session id"));
            }

            if is_controling_terminal {
                set_controlling_terminal(child)?;
            }

            // No longer need child/main fds.
            libc::close(child);
            libc::close(main);

            libc::signal(libc::SIGCHLD, libc::SIG_DFL);
            libc::signal(libc::SIGHUP, libc::SIG_DFL);
            libc::signal(libc::SIGINT, libc::SIG_DFL);
            libc::signal(libc::SIGQUIT, libc::SIG_DFL);
            libc::signal(libc::SIGTERM, libc::SIG_DFL);
            libc::signal(libc::SIGALRM, libc::SIG_DFL);

            Ok(())
        });
    }

    // Handle set working directory option.
    if let Some(dir) = &working_directory {
        builder.current_dir(dir);
    }

    // Prepare signal handling before spawning child.
    let signals =
        Signals::new([sigconsts::SIGCHLD]).expect("error preparing signal handling");

    match builder.spawn() {
        Ok(child_process) => {
            unsafe {
                set_nonblocking(main);
            }

            let ptsname: String = tty_ptsname(main).unwrap_or_else(|_| "".to_string());
            let child_unix = Child {
                id: Arc::new(main),
                ptsname,
                pid: Arc::new(child_process.id().try_into().unwrap()),
                process: Some(child_process),
            };

            Ok(Pty {
                child: child_unix,
                file: unsafe { File::from_raw_fd(main) },
                token: corcovado::Token::from(0),
                signals,
                signals_token: corcovado::Token::from(0),
            })
        }
        Err(err) => Err(Error::new(
            err.kind(),
            format!(
                "Failed to spawn command '{}': {}",
                builder.get_program().to_string_lossy(),
                err
            ),
        )),
    }
}

///
/// Creates a pseudoterminal using fork.
///
/// The [`create_pty`] creates a pseudoterminal with similar behavior as tty,
/// which is a command in Unix and Unix-like operating systems to print the file name of the
/// terminal connected to standard input. tty stands for TeleTYpewriter.
///
/// It returns two [`Pty`] along with respective process name [`String`] and process id (`libc::pid_`)
///
pub fn create_pty_with_fork(shell: &str, columns: u16, rows: u16) -> Result<Pty, Error> {
    let mut main = 0;
    let winsize = Winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: columns as libc::c_ushort,
        ws_width: 0 as libc::c_ushort,
        ws_height: 0 as libc::c_ushort,
    };
    let term = create_termp(true);

    let mut shell_program = shell;

    let user = match ShellUser::from_env() {
        Ok(data) => data,
        Err(..) => ShellUser {
            shell: shell.to_string(),
            ..Default::default()
        },
    };

    if shell.is_empty() {
        tracing::info!("shell configuration is empty, will retrieve from env");
        shell_program = &user.shell;
    }

    tracing::info!("fork {:?}", shell_program);

    match unsafe {
        forkpty(
            &mut main as *mut _,
            ptr::null_mut(),
            &term as *const libc::termios,
            &winsize as *const _,
        )
    } {
        0 => {
            default_shell_command(shell_program);
            Err(Error::other(format!(
                "forkpty has reach unreachable with {shell_program}"
            )))
        }
        id if id > 0 => {
            // TODO: Currently we fork the process and don't wait to know if led to failure
            // Whenever it happens it will just simply shut down the teletyperwriter
            // In the future add an option to check before release the method
            let ptsname: String = tty_ptsname(main).unwrap_or_else(|_| "".to_string());
            let child = Child {
                id: Arc::new(main),
                ptsname,
                pid: Arc::new(id),
                process: None,
            };

            unsafe {
                set_nonblocking(main);
            }

            let signals = Signals::new([sigconsts::SIGCHLD])
                .expect("error preparing signal handling");
            Ok(Pty {
                child,
                signals,
                file: unsafe { File::from_raw_fd(main) },
                token: corcovado::Token(0),
                signals_token: corcovado::Token(0),
            })
        }
        _ => Err(Error::other(format!(
            "forkpty failed using {shell_program}"
        ))),
    }
}

/// Really only needed on BSD, but should be fine elsewhere.
fn set_controlling_terminal(fd: libc::c_int) -> Result<(), Error> {
    let res = unsafe {
        // TIOSCTTY changes based on platform and the `ioctl` call is different
        // based on architecture (32/64). So a generic cast is used to make sure
        // there are no issues. To allow such a generic cast the clippy warning
        // is disabled.
        #[allow(clippy::cast_lossless)]
        libc::ioctl(fd, libc::TIOCSCTTY as _, 0)
    };

    if res < 0 {
        return Err(Error::last_os_error());
    }

    Ok(())
}

// https://man7.org/linux/man-pages/man2/fcntl.2.html
unsafe fn set_nonblocking(fd: libc::c_int) {
    use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

    let res = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    assert_eq!(res, 0);
}

#[derive(Debug)]
pub struct Child {
    pub id: Arc<libc::c_int>,
    pub pid: Arc<libc::pid_t>,
    #[allow(dead_code)]
    ptsname: String,
    #[allow(dead_code)]
    process: Option<std::process::Child>,
}

impl Child {
    /// The tcgetwinsize function fills in the winsize structure pointed to by
    ///  gws with values that represent the size of the terminal window for which
    ///  fd provides an open file descriptor.  If no error occurs tcgetwinsize()
    ///  returns zero (0).
    ///  The tcsetwinsize function sets the terminal window size, for the terminal
    ///  referenced by fd, to the sizes from the winsize structure pointed to by
    ///  sws.  If no error occurs tcsetwinsize() returns zero (0).
    ///  The winsize structure, defined in <termios.h>, contains (at least) the
    ///  following four fields
    ///  unsigned short ws_row;      /* Number of rows, in characters */
    ///  unsigned short ws_col;      /* Number of columns, in characters */
    ///  unsigned short ws_xpixel;   /* Width, in pixels */
    ///  unsigned short ws_ypixel;   /* Height, in pixels */
    /// If the actual window size of the controlling terminal of a process
    /// changes, the process is sent a SIGWINCH signal.  See signal(7).  Note
    /// simply changing the sizes using tcsetwinsize() does not necessarily
    /// change the actual window size, and if not, will not generate a SIGWINCH.
    pub fn set_winsize(&self, winsize_builder: WinsizeBuilder) -> io::Result<()> {
        let winsize: Winsize = winsize_builder.build();
        match unsafe { libc::ioctl(**self, TIOCSWINSZ, &winsize as *const _) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    /// Return the child’s exit status if it has already exited. If the child is still running, return Ok(None).
    /// https://linux.die.net/man/2/waitpid
    pub fn waitpid(&self) -> Result<Option<i32>, String> {
        let mut status = 0 as libc::c_int;
        // If WNOHANG was specified in options and there were no children in a waitable state, then waitid() returns 0 immediately and the state of the siginfo_t structure pointed to by infop is unspecified. To distinguish this case from that where a child was in a waitable state, zero out the si_pid field before the call and check for a nonzero value in this field after the call returns.
        let res =
            unsafe { waitpid(*self.pid, &mut status as *mut libc::c_int, libc::WNOHANG) };
        if res <= -1 {
            return Err(String::from("error"));
        }

        if res == 0 && status == 0 {
            return Ok(None);
        }

        Ok(Some(status))
    }

    pub fn close(&self) {
        unsafe {
            libc::close(*self.pid);
        }
    }
}

pub fn kill_pid(pid: i32) {
    unsafe {
        libc::kill(pid, libc::SIGHUP);
    }
}

impl Deref for Child {
    type Target = libc::c_int;
    fn deref(&self) -> &libc::c_int {
        &self.id
    }
}

impl Drop for Child {
    fn drop(&mut self) {
        unsafe {
            libc::kill(*self.pid, libc::SIGHUP);
        }
    }
}

pub fn command_per_pid(pid: libc::pid_t) -> String {
    let current_process_name = Command::new("ps")
        .arg("-p")
        .arg(format!("{pid:}"))
        .arg("-o")
        .arg("comm=")
        .output()
        .expect("failed to execute process")
        .stdout;

    std::str::from_utf8(&current_process_name)
        .unwrap_or("")
        .to_string()
}

impl EventedPty for Pty {
    #[inline]
    fn next_child_event(&mut self) -> Option<ChildEvent> {
        self.signals.pending().next().and_then(|signal| {
            if signal != sigconsts::SIGCHLD {
                return None;
            }

            match self.child.waitpid() {
                Err(_e) => {
                    // std::process::exit(1);
                    None
                }
                Ok(None) => None,
                Ok(Some(..)) => Some(ChildEvent::Exited),
            }
        })
    }

    #[inline]
    fn child_event_token(&self) -> corcovado::Token {
        self.signals_token
    }
}

#[derive(Debug)]
struct Passwd<'a> {
    name: &'a str,
    dir: &'a str,
    shell: &'a str,
}

/// Return a Passwd struct with pointers into the provided buf.
///
/// # Unsafety
///
/// If `buf` is changed while `Passwd` is alive, bad thing will almost certainly happen.
fn get_pw_entry(buf: &mut [i8; 1024]) -> Result<Passwd<'_>, Error> {
    // Create zeroed passwd struct.
    let mut entry: MaybeUninit<libc::passwd> = MaybeUninit::uninit();

    let mut res: *mut libc::passwd = ptr::null_mut();

    // Try and read the pw file.
    let uid = unsafe { libc::getuid() };
    let status = unsafe {
        libc::getpwuid_r(
            uid,
            entry.as_mut_ptr(),
            buf.as_mut_ptr() as *mut _,
            buf.len(),
            &mut res,
        )
    };
    let entry = unsafe { entry.assume_init() };

    if status < 0 {
        return Err(Error::other("getpwuid_r failed"));
    }

    if res.is_null() {
        return Err(Error::other("pw not found"));
    }

    // Sanity check.
    assert_eq!(entry.pw_uid, uid);

    // Build a borrowed Passwd struct.
    Ok(Passwd {
        name: unsafe { CStr::from_ptr(entry.pw_name).to_str().unwrap() },
        dir: unsafe { CStr::from_ptr(entry.pw_dir).to_str().unwrap() },
        shell: unsafe { CStr::from_ptr(entry.pw_shell).to_str().unwrap() },
    })
}

/// Unsafe
/// Return tty pts name [`String`]
///
/// # Safety
///
/// This function is unsafe because it contains the usage of `libc::ptsname`
/// from libc that's naturally unsafe.
pub fn tty_ptsname(fd: libc::c_int) -> Result<String, String> {
    let c_str: &CStr = unsafe {
        let name_ptr = ptsname(fd as *mut _);
        CStr::from_ptr(name_ptr)
    };
    let str_slice: &str = c_str.to_str().unwrap();
    let str_buf: String = str_slice.to_owned();

    Ok(str_buf)
}

pub fn foreground_process_name(main_fd: RawFd, shell_pid: u32) -> String {
    let mut pid = unsafe { libc::tcgetpgrp(main_fd) };
    if pid < 0 {
        pid = shell_pid as libc::pid_t;
    }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    let comm_path = format!("/proc/{pid}/comm");
    #[cfg(target_os = "freebsd")]
    let comm_path = format!("/compat/linux/proc/{pid}/comm");

    #[cfg(not(target_os = "macos"))]
    let name = match std::fs::read(comm_path) {
        Ok(comm_str) => String::from_utf8_lossy(&comm_str)
            .trim_end()
            .parse()
            .unwrap_or_default(),
        Err(..) => String::from(""),
    };

    #[cfg(target_os = "macos")]
    let name = macos_process_name(pid);

    name
}

pub fn foreground_process_path(
    main_fd: RawFd,
    shell_pid: u32,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let mut pid = unsafe { libc::tcgetpgrp(main_fd) };
    if pid < 0 {
        pid = shell_pid as libc::pid_t;
    }

    #[cfg(not(any(target_os = "macos", target_os = "freebsd")))]
    let link_path = format!("/proc/{pid}/cwd");
    #[cfg(target_os = "freebsd")]
    let link_path = format!("/compat/linux/proc/{pid}/cwd");

    #[cfg(not(target_os = "macos"))]
    let cwd = std::fs::read_link(link_path)?;

    #[cfg(target_os = "macos")]
    let cwd = macos_cwd(pid)?;

    Ok(cwd)
}

/// Start a new process in the background.
pub fn spawn_daemon<I, S>(
    program: &str,
    args: I,
    main_fd: RawFd,
    shell_pid: u32,
) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<std::ffi::OsStr>,
{
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(cwd) = foreground_process_path(main_fd, shell_pid) {
        command.current_dir(cwd);
    }
    unsafe {
        command
            .pre_exec(|| {
                match libc::fork() {
                    -1 => return Err(io::Error::last_os_error()),
                    0 => (),
                    _ => libc::_exit(0),
                }

                if libc::setsid() == -1 {
                    return Err(io::Error::last_os_error());
                }

                Ok(())
            })
            .spawn()?
            .wait()
            .map(|_| ())
    }
}
