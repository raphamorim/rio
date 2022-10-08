#![deny(clippy::all)]

use std::ffi::{CStr, CString};
use std::ptr::{null, null_mut};

use nix::libc::openpty;
use nix::libc::B38400;
use nix::libc::*;
use nix::libc::{cfsetispeed, cfsetospeed};
use nix::libc::{fcntl, forkpty, sigfillset, termios};
use nix::libc::{winsize, O_NONBLOCK};
use nix::sys::signal::Signal;

use nix::errno::Errno;
use nix::unistd::chdir;

#[derive(Debug)]
pub struct Process {
    pub fd: i32,
    pub pid: i32,
    pub pty: String,
}

#[derive(Debug)]
pub struct OpenProcess {
    pub main: i32,
    pub branch: i32,
    pub pty: String,
}

pub fn fork(
    file: String,
    args: Vec<String>,
    env: Vec<String>,
    cwd: String,
    cols: i32,
    rows: i32,
    uid: i32,
    gid: i32,
    utf8: bool,
) -> Result<Process, String> {
    //
    let mut newmask: sigset_t = 0;
    let mut oldmask: sigset_t = 0;
    //
    let mut sig_action = sigaction {
        sa_sigaction: SIG_DFL,
        sa_mask: 0,
        sa_flags: 0,
    };

    // Terminal window size
    let winp = winsize {
        ws_col: cols as u16,
        ws_row: rows as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    // Create a new termios with default flags.
    // For more info on termios settings:
    // https://man7.org/linux/man-pages/man3/termios.3.html
    let mut term = termios {
        c_iflag: ICRNL | IXON | IXANY | IMAXBEL | BRKINT,
        c_oflag: OPOST | ONLCR,
        c_cflag: CREAD | CS8 | HUPCL,
        c_lflag: ICANON | ISIG | IEXTEN | ECHO | ECHOE | ECHOK | ECHOKE | ECHOCTL,
        c_cc: Default::default(),
        c_ispeed: Default::default(),
        c_ospeed: Default::default(),
    };

    // Enable utf8 support if requested
    if utf8 {
        term.c_iflag |= IUTF8;
    }

    // Set supported terminal characters
    term.c_cc[VEOF] = 4;
    term.c_cc[VEOL] = 255;
    term.c_cc[VEOL2] = 255;
    term.c_cc[VERASE] = 0x7f;
    term.c_cc[VWERASE] = 23;
    term.c_cc[VKILL] = 21;
    term.c_cc[VREPRINT] = 18;
    term.c_cc[VINTR] = 3;
    term.c_cc[VQUIT] = 0x1c;
    term.c_cc[VSUSP] = 26;
    term.c_cc[VSTART] = 17;
    term.c_cc[VSTOP] = 19;
    term.c_cc[VLNEXT] = 22;
    term.c_cc[VDISCARD] = 15;
    term.c_cc[VMIN] = 1;
    term.c_cc[VTIME] = 0;

    // Specific character support for macos
    #[cfg(target_os = "macos")]
    {
        term.c_cc[VDSUSP] = 25;
        term.c_cc[VSTATUS] = 20;
    }

    unsafe {
        // Set terminal input and output baud rate
        cfsetispeed(&mut term, B38400);
        cfsetospeed(&mut term, B38400);

        // temporarily block all signals
        // this is needed due to a race condition in openpty
        // and to avoid running signal handlers in the child
        // before exec* happened
        sigfillset(&mut newmask);
        pthread_sigmask(SIG_SETMASK, &mut newmask, &mut oldmask);
    }

    // Forks and then assigns a pointer to the fork file descriptor to main
    let mut main: i32 = -1;
    let pid = pty_forkpty(&mut main, term, winp);

    if pid == 0 {
        // remove all signal handlers from child
        sig_action.sa_sigaction = SIG_DFL;
        sig_action.sa_flags = 0;
        unsafe {
            sigemptyset(&mut sig_action.sa_mask);
            for i in Signal::iterator() {
                sigaction(i as c_int, &sig_action, null_mut());
            }
        }
    }

    // Reenable signals
    unsafe {
        pthread_sigmask(SIG_SETMASK, &mut oldmask, null_mut());
    }

    match pid {
        -1 => {
            panic!("forkpty(3) failed.")
        }
        0 => {
            unsafe {
                if !cwd.is_empty() {
                    if chdir(cwd.as_str()).is_err() {
                        child_panic("chdir(2) failed");
                    }
                }

                if uid != -1 && gid != -1 {
                    if setgid(gid as u32) == -1 {
                        child_panic("setgid(2) failed");
                    }
                    if setuid(uid as u32) == -1 {
                        child_panic("setuid(2) failed");
                    }
                }
                // Prepare char *argv[]: [file, ...args, null]
                let cargs = vec![&file]
                    .into_iter()
                    .chain(args.iter())
                    .map(|s| cstr_unsafe(s.clone()))
                    .collect::<Vec<_>>();
                let argv = nul_terminated(&cargs);

                let cenv = env
                    .iter()
                    .map(|s| cstr_unsafe(s.clone()))
                    .collect::<Vec<_>>();
                let envv = nul_terminated(&cenv);

                let fptr = match CString::new(file) {
                  Ok(d) => {
                    d.as_ptr()
                  }
                  Err(_) => {
                    panic!("failed to prepare fptr")
                  }
                };

                pty_execvpe(fptr, argv.as_ptr(), envv.as_ptr());

                child_panic("execvp(3) failed");
            }
        }
        _ => unsafe {
            pty_nonblock(main);
        },
    };

    let pty = unsafe { pty_ptsname(main).expect("ptsname failed") };
    return Ok(Process {
        fd: main,
        pid,
        pty,
    });
}

fn cstr_unsafe(s: String) -> CString {
    CString::new(s).expect("CString::new failed")
}
fn cstr_unsafe_(s: &str) -> CString {
    CString::new(s).expect("CString::new failed")
}

fn nul_terminated(arr: &Vec<CString>) -> Vec<*const c_char> {
    arr.iter()
        .map(|s| s.as_ptr())
        .chain(vec![null()].into_iter())
        .collect::<Vec<_>>()
}

fn child_panic(s: &str) {
    unsafe {
        perror(cstr_unsafe_(s).as_ptr());
        exit(1);
    }
}

/// Passes the call to the unsafe function forkpty
#[cfg(target_os = "macos")]
pub fn pty_forkpty(main: &mut i32, mut termp: termios, mut winp: winsize) -> i32 {
    unsafe { forkpty(main, null_mut::<c_char>(), &mut termp, &mut winp) }
}

/// Get's the name of the terminal pointed to by the given file descriptor
unsafe fn pty_ptsname(main: c_int) -> nix::Result<String> {
    let name_ptr = ptsname(main);
    if name_ptr.is_null() {
        return Err(Errno::last());
    }

    let name = CStr::from_ptr(name_ptr);
    Ok(name.to_string_lossy().into_owned())
}

/// execvpe(3) is not portable.
/// http://www.gnu.org/software/gnulib/manual/html_node/execvpe.html
unsafe fn pty_execvpe(
    file: *const i8,
    argv: *const *const i8,
    envp: *const *const i8,
) -> i32 {
    /* this is the hackiest, but that's what used to be in the C++ implementation */
    extern "C" {
        static mut environ: *const *const i8;
    }
    environ = envp;
    /* suggestion: pass envp as Vec<String> and use
     *   nix::env::clearenv();
     *   std::env::setenv(...);
     * also, optimization: change `unixTerminal.ts` to pass `undefined` in case
     * an `env` option is not set. And then, in this case, skip this charade
     * altogether.
     */
    return execvp(file, argv);
}

unsafe fn pty_nonblock(fd: c_int) -> c_int {
    match fcntl(fd, F_GETFL, 0) {
        -1 => panic!("failed to set nonblocking mode (fcntl(F_GETFL) failed)"),
        flags => match fcntl(fd, F_SETFL, flags | O_NONBLOCK) {
            -1 => panic!("failed to set nonblocking mode (fcntl(F_SETFL) failed)"),
            rc => rc,
        },
    }
}

unsafe fn pty_waitpid(pid: pid_t) -> (u32, u32) {
    let mut stat_loc: c_int = 0;
    let ret = waitpid(pid, &mut stat_loc, 0);
    match ret {
        -1 => match Errno::last() {
            Errno::EINTR => pty_waitpid(pid),
            Errno::ECHILD => (0, 0),
            _ => panic!("waitpid(3): unexpected error"),
        },
        _ => (
            if WIFEXITED(stat_loc) {
                WEXITSTATUS(stat_loc) as u32
            } else {
                0
            },
            if WIFSIGNALED(stat_loc) {
                WTERMSIG(stat_loc) as u32
            } else {
                0
            },
        ),
    }
}

pub fn open(cols: u32, rows: u32) -> OpenProcess {
    // Terminal window size
    let mut winp = winsize {
        ws_col: cols as u16,
        ws_row: rows as u16,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let mut amain: i32 = 0;
    let mut abranch: i32 = 0;
    unsafe {
        openpty(
            &mut amain,
            &mut abranch,
            null::<i8>() as *mut i8,
            null::<i8>() as *mut termios,
            &mut winp,
        );
    }

    OpenProcess {
        main: amain,
        branch: abranch,
        pty: String::new(),
    }
}
