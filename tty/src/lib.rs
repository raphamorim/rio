extern crate libc;

use std::ffi::{CStr, CString};
use std::io;
use std::ops::Deref;
use std::ptr;
use std::sync::Arc;

pub static COLS: u32 = 80;
pub static ROWS: u32 = 30;

#[cfg(target_os = "linux")]
const TIOCSWINSZ: libc::c_ulong = 0x5414;
#[cfg(target_os = "macos")]
const TIOCSWINSZ: libc::c_ulong = 2148037735;

#[repr(C)]
struct Winsize {
    ws_row: libc::c_ushort,
    ws_col: libc::c_ushort,
    ws_xpixel: libc::c_ushort,
    ws_ypixel: libc::c_ushort,
}

#[link(name = "util")]
extern "C" {
    fn forkpty(
        main: *mut libc::c_int,
        name: *mut libc::c_char,
        termp: *const libc::termios,
        winsize: *const Winsize,
    ) -> libc::pid_t;

    fn ptsname(fd: *mut libc::c_int) -> *mut libc::c_char;
}

#[derive(Debug)]
pub struct Process(Handle);

impl Deref for Process {
    type Target = Handle;
    fn deref(&self) -> &Handle {
        &self.0
    }
}

impl io::Write for Process {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match unsafe {
            libc::write(*self.0, buf.as_ptr() as *const _, buf.len() as libc::size_t)
        } {
            n if n >= 0 => Ok(n as usize),
            _ => Err(io::Error::last_os_error()),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl io::Read for Process {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match unsafe {
            libc::read(
                *self.0,
                buf.as_mut_ptr() as *mut _,
                buf.len() as libc::size_t,
            )
        } {
            n if n >= 0 => Ok(n as usize),
            _ => Err(io::Error::last_os_error()),
        }
    }
}

pub fn create_termp(utf8: bool) -> libc::termios {
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

pub fn pty(name: &str, width: u16, height: u16) -> (Process, Process, String) {
    let mut main = 0;
    let winsize = Winsize {
        ws_row: height as libc::c_ushort,
        ws_col: width as libc::c_ushort,
        ws_xpixel: 0,
        ws_ypixel: 0,
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
            let name = CString::new(name).unwrap();
            unsafe {
                libc::execvp(name.as_ptr(), ptr::null());
            }
            unreachable!();
        }
        n if n > 0 => {
            let pid: String;
            unsafe {
                pid = tty_ptsname(main).unwrap_or_else(|_| "".to_string());
            }
            let handle = Handle(Arc::new(main));
            (Process(handle.clone()), Process(handle), pid)
        }
        _ => panic!("Fork failed."),
    }
}

#[derive(Debug, Clone)]
pub struct Handle(Arc<libc::c_int>);

impl Handle {
    pub fn set_winsize(&self, width: u16, height: u16) -> io::Result<()> {
        let winsize = Winsize {
            ws_row: height as libc::c_ushort,
            ws_col: width as libc::c_ushort,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        match unsafe { libc::ioctl(**self, TIOCSWINSZ, &winsize as *const _) } {
            -1 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }
}

impl Deref for Handle {
    type Target = libc::c_int;
    fn deref(&self) -> &libc::c_int {
        &self.0
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            libc::close(*self.0);
        }
    }
}

unsafe fn tty_ptsname(fd: libc::c_int) -> Result<String, String> {
    let name_ptr = ptsname(fd as *mut _);
    let c_str: &CStr = CStr::from_ptr(name_ptr);
    let str_slice: &str = c_str.to_str().unwrap();
    let str_buf: String = str_slice.to_owned();

    Ok(str_buf)
}

pub struct TtyConfig {
    env: Vec<(String, String)>
}

pub fn setup_env() {
// pub fn setup_env(config: &TtyConfig) {
    let terminfo = if terminfo_exists("rio") { "rio" } else { "xterm-256color" };
    std::env::set_var("TERM", terminfo);

    std::env::set_var("COLORTERM", "truecolor");
    std::env::remove_var("DESKTOP_STARTUP_ID");

    // Set env vars from config.
    // for (key, value) in config.env.iter() {
        // std::env::set_var(key, value);
    // }
}

/// Check if a terminfo entry exists on the system.
fn terminfo_exists(terminfo: &str) -> bool {
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
        check_path!(std::path::PathBuf::from(&dir));
    } 
    // else if let Some(home) = dirs::home_dir() {
    //     check_path!(home.join(".terminfo"));
    // }

    // if let Ok(dirs) = std::env::var("TERMINFO_DIRS") {
    //     for dir in dirs.split(':') {
    //         check_path!(std::path::PathBuf::from(dir));
    //     }
    // }

    if let Ok(prefix) = std::env::var("PREFIX") {
        let path = std::path::PathBuf::from(prefix);
        check_path!(path.join("etc/terminfo"));
        check_path!(path.join("lib/terminfo"));
        check_path!(path.join("share/terminfo"));
    }

    check_path!(std::path::PathBuf::from("/etc/terminfo"));
    check_path!(std::path::PathBuf::from("/lib/terminfo"));
    check_path!(std::path::PathBuf::from("/usr/share/terminfo"));
    check_path!(std::path::PathBuf::from("/boot/system/data/terminfo"));

    // No valid terminfo path has been found.
    false
}