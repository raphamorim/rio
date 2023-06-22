#![cfg(unix)]

mod signals;
mod socket;
mod stream;

extern crate libc;

use crate::{ChildEvent, EventedPty, ProcessReadWrite, Winsize, WinsizeBuilder};
use signal_hook::consts as sigconsts;
use signals::Signals;
use std::ffi::{CStr, CString};
use std::fs::File;
use std::io;
use std::ops::Deref;
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::path::PathBuf;
use std::process::Command;
use std::ptr;
use std::sync::Arc;
use urca::unix::EventedFd;

#[cfg(target_os = "linux")]
const TIOCSWINSZ: libc::c_ulong = 0x5414;
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
    token: urca::Token,
    signals_token: urca::Token,
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
    fn read_token(&self) -> urca::Token {
        self.token
    }

    #[inline]
    fn writer(&mut self) -> &mut File {
        &mut self.file
    }

    #[inline]
    fn write_token(&self) -> urca::Token {
        self.token
    }

    #[inline]
    fn set_winsize(&mut self, winsize: WinsizeBuilder) -> Result<(), std::io::Error> {
        self.child.set_winsize(winsize)
    }

    #[inline]
    fn register(
        &mut self,
        poll: &urca::Poll,
        token: &mut dyn Iterator<Item = urca::Token>,
        interest: urca::Ready,
        poll_opts: urca::PollOpt,
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
            urca::Ready::readable(),
            urca::PollOpt::level(),
        )
    }

    fn reregister(
        &mut self,
        poll: &urca::Poll,
        interest: urca::Ready,
        poll_opts: urca::PollOpt,
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
            urca::Ready::readable(),
            urca::PollOpt::level(),
        )
    }

    fn deregister(&mut self, poll: &urca::Poll) -> io::Result<()> {
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
        c_ispeed: Default::default(),
        c_ospeed: Default::default(),
        c_line: 0,
    };

    #[cfg(target_os = "macos")]
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

    // Enable utf8 support if requested
    if utf8 {
        term.c_iflag |= libc::IUTF8;
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

///
/// Creates a pseudoterminal.
///
/// The [`create_pty`] creates a pseudoterminal with similar behavior as tty,
/// which is a command in Unix and Unix-like operating systems to print the file name of the
/// terminal connected to standard input. tty stands for TeleTYpewriter.
///
/// It returns two [`Pty`] along with respective process name [`String`] and process id (`libc::pid_`)
///
pub fn create_pty(shell: &str, columns: u16, rows: u16) -> Pty {
    let mut main = 0;
    let winsize = Winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: columns as libc::c_ushort,
        ws_width: 0 as libc::c_ushort,
        ws_height: 0 as libc::c_ushort,
    };
    let term = create_termp(true);

    match unsafe {
        forkpty(
            &mut main as *mut _,
            ptr::null_mut(),
            &term as *const libc::termios,
            &winsize as *const _,
        )
    } {
        0 => {
            default_shell_command(shell);
            unreachable!();
        }
        id if id > 0 => {
            let ptsname: String = tty_ptsname(main).unwrap_or_else(|_| "".to_string());
            let child = Child {
                id: Arc::new(main),
                ptsname,
                pid: Arc::new(id),
            };

            unsafe {
                set_nonblocking(main);
            }

            let signals = Signals::new([sigconsts::SIGCHLD])
                .expect("error preparing signal handling");
            Pty {
                child,
                signals,
                file: unsafe { File::from_raw_fd(main) },
                token: urca::Token(0),
                signals_token: urca::Token(0),
            }
        }
        _ => panic!("Fork failed."),
    }
}

// https://man7.org/linux/man-pages/man2/fcntl.2.html
unsafe fn set_nonblocking(fd: libc::c_int) {
    use libc::{fcntl, F_GETFL, F_SETFL, O_NONBLOCK};

    let res = fcntl(fd, F_SETFL, fcntl(fd, F_GETFL, 0) | O_NONBLOCK);
    assert_eq!(res, 0);
}

#[derive(Debug, Clone)]
pub struct Child {
    id: Arc<libc::c_int>,
    #[allow(dead_code)]
    ptsname: String,
    pid: Arc<libc::pid_t>,
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
    fn child_event_token(&self) -> urca::Token {
        self.signals_token
    }
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
