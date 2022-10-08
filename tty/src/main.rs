use std::io;
use std::ffi::CString;
use std::ops::Deref;
use std::ptr;
use std::borrow::Cow;

use std::env;
use std::io::BufReader;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};


use std::thread;

extern crate libc;

static COLS: u32 = 80;
static ROWS: u32 = 25;

#[cfg(target_os="linux")]
const TIOCSWINSZ: libc::c_ulong = 0x5414;
#[cfg(target_os="macos")]
const TIOCSWINSZ: libc::c_ulong = 2148037735;

#[link(name="util")]
extern {
    fn forkpty(amaster: *mut libc::c_int,
               name: *mut libc::c_char,
               termp: *const libc::c_void,
               winsize: *const Winsize) -> libc::pid_t;
}

#[repr(C)]
struct Winsize {
    ws_row: libc::c_ushort,
    ws_col: libc::c_ushort,
    ws_xpixel: libc::c_ushort,
    ws_ypixel: libc::c_ushort,
}

pub fn pty(name: &str, width: u16, height: u16) -> (Reader, Writer) {
    let mut amaster = 0;
    let winsize = Winsize {
        ws_row: height as libc::c_ushort,
        ws_col: width as libc::c_ushort,
        ws_xpixel: 0,
        ws_ypixel: 0
    };
    match unsafe {
        forkpty(&mut amaster as *mut _,
                ptr::null_mut(),
                ptr::null(),
                &winsize as *const _)
    } {
        0           => {
            let name = CString::new(name).unwrap();
            unsafe {
                libc::execvp(name.as_ptr(), ptr::null());
            }
            unreachable!();
        }
        n if n > 0  => {
            let handle = Arc::new(Handle(amaster));
            (Reader(handle.clone()), Writer(handle.clone()))
        }
        _           => panic!("Fork failed.")
    }
}

#[derive(Debug)]
pub struct Reader(Arc<Handle>);

impl Deref for Reader {
    type Target = Handle;
    fn deref(&self) -> &Handle {
        &*self.0
    }
}

impl io::Read for Reader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match unsafe {
            libc::read(**self.0,
                       buf.as_mut_ptr() as *mut _,
                       buf.len() as libc::size_t)
        } {
            n if n >= 0 => Ok(n as usize),
            _           => Err(io::Error::last_os_error()),
        }
    }
}

pub struct Writer(Arc<Handle>);

impl Deref for Writer {
    type Target = Handle;
    fn deref(&self) -> &Handle {
        &*self.0
    }
}

impl io::Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match unsafe {
            libc::write(**self.0,
                        buf.as_ptr() as *const _,
                        buf.len() as libc::size_t)
        } {
            n if n >= 0 => Ok(n as usize),
            _           => Err(io::Error::last_os_error()),
        }
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

// impl Tty for Writer {
//     fn set_winsize(&mut self, width: u16, height: u16) -> io::Result<()> {
//         (**self).set_winsize(width, height)
//     }
// }

#[derive(Debug)]
pub struct Handle(libc::c_int);

impl Handle {
    pub fn set_winsize(&self, width: u16, height: u16) -> io::Result<()> {
        let winsize = Winsize {
            ws_row: height as libc::c_ushort,
            ws_col: width as libc::c_ushort,
            ws_xpixel: 0,
            ws_ypixel: 0
        };
        match unsafe {
            libc::ioctl(**self,
                        TIOCSWINSZ,
                        &winsize as *const _)
        } {
            -1  => Err(io::Error::last_os_error()),
            _   => Ok(()),
        }
    }
}

impl Deref for Handle {
    type Target = libc::c_int;
    fn deref(&self) -> &libc::c_int { &self.0 }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

// pub struct Output<R: io::BufRead> {
//     pub struct Output<R: io::BufRead> {
//     tty: io::Chars<R>,
// }

// impl<R: io::BufRead> Output<R> {

//     /// Create a new output processor wrapping a buffered read interface to the tty.
//     pub fn new(tty: R) -> Output<R> {
//         Output {
//             tty: tty.chars(),
//         }
//     }
// }

fn main() {
    // Set the TERM variable and establish a TTY connection
    env::set_var("TERM", "rio");

    let shell = Cow::Borrowed("zsh");
    let (tty_r, _tty_w) = pty(&shell, COLS as u16, ROWS as u16);

    // Handle program output (tty -> screen) on separate thread.
    // let (tx_out, rx) = mpsc::channel();
    // let (tx_key_press, tx_key_release) = (tx_out.clone(), tx_out.clone());

    let pty_open = Arc::new(AtomicBool::new(true));
    let _pty_open_checker = pty_open.clone();
    thread::spawn(move || {
        // let output = Output::new(BufReader::new(tty_r));
        let reader = BufReader::new(tty_r);
        // for result in output {
        // for line in output {
        //     match result {
        //         Ok(cmd) => {
                    // println!("111{:?}", reader);
                    // println!("{:?}", reader.chars());
                // },
                // Err(_) => {
                //     break;
                // },
            // }
        // }
        pty_open.store(false, Ordering::SeqCst);
    });

    // let w: Box<Writer> = tty_w);
    // tty_w.write(String::from("ls").as_bytes());

    loop {
        // println!("{:?}", tty_r);

    }
}