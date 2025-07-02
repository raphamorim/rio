// From: https://github.com/alacritty/alacritty/blob/04ea367e3baa7e51933e9a595da793b4c8a4aa8f/alacritty/src/macos/proc.rs

use std::ffi::{CStr, CString, IntoStringError};
use std::fmt::{self, Display, Formatter};
use std::io;
use std::mem::MaybeUninit;
use std::os::raw::c_int;
use std::path::PathBuf;

/// Error during working directory retrieval.
#[derive(Debug)]
pub enum Error {
    Io(io::Error),

    /// Error converting into utf8 string.
    IntoString(IntoStringError),

    /// Expected return size didn't match libproc's.
    InvalidSize,
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::InvalidSize => None,
            Error::Io(err) => err.source(),
            Error::IntoString(err) => err.source(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidSize => write!(f, "Invalid proc_pidinfo return size"),
            Error::Io(err) => {
                write!(f, "Error getting current working directory: {err}")
            }
            Error::IntoString(err) => {
                write!(f, "Error when parsing current working directory: {err}")
            }
        }
    }
}

impl From<io::Error> for Error {
    fn from(val: io::Error) -> Self {
        Error::Io(val)
    }
}

impl From<IntoStringError> for Error {
    fn from(val: IntoStringError) -> Self {
        Error::IntoString(val)
    }
}

/// Bindings for libproc.
#[allow(non_camel_case_types)]
mod sys {
    use std::os::raw::{c_char, c_int, c_longlong, c_void};

    pub const PROC_PIDVNODEPATHINFO: c_int = 9;

    type gid_t = c_int;
    type off_t = c_longlong;
    type uid_t = c_int;
    type fsid_t = fsid;

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct fsid {
        pub val: [i32; 2usize],
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct vinfo_stat {
        pub vst_dev: u32,
        pub vst_mode: u16,
        pub vst_nlink: u16,
        pub vst_ino: u64,
        pub vst_uid: uid_t,
        pub vst_gid: gid_t,
        pub vst_atime: i64,
        pub vst_atimensec: i64,
        pub vst_mtime: i64,
        pub vst_mtimensec: i64,
        pub vst_ctime: i64,
        pub vst_ctimensec: i64,
        pub vst_birthtime: i64,
        pub vst_birthtimensec: i64,
        pub vst_size: off_t,
        pub vst_blocks: i64,
        pub vst_blksize: i32,
        pub vst_flags: u32,
        pub vst_gen: u32,
        pub vst_rdev: u32,
        pub vst_qspare: [i64; 2usize],
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone)]
    pub struct vnode_info {
        pub vi_stat: vinfo_stat,
        pub vi_type: c_int,
        pub vi_pad: c_int,
        pub vi_fsid: fsid_t,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct vnode_info_path {
        pub vip_vi: vnode_info,
        pub vip_path: [c_char; 1024usize],
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    pub struct proc_vnodepathinfo {
        pub pvi_cdir: vnode_info_path,
        pub pvi_rdir: vnode_info_path,
    }

    extern "C" {
        pub fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;

        pub fn proc_pidinfo(
            pid: c_int,
            flavor: c_int,
            arg: u64,
            buffer: *mut c_void,
            buffersize: c_int,
        ) -> c_int;

        // pub fn proc_name(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    }
}

pub fn macos_process_name(pid: libc::c_int) -> String {
    let mut name = String::new();

    if pid >= 0 {
        let proc_path = get_proc_path(pid);
        name = std::path::Path::new(&proc_path)
            .file_name()
            .unwrap_or(std::ffi::OsStr::new(""))
            .to_str()
            .unwrap_or("")
            .to_string();
    }

    //     // proc_name truncates the name to 16 bytes.
    //     let mut name_buf: Vec<u8> = Vec::with_capacity(16);    // MAXCOMLEN (max command name remembered)
    //     let mut var_int_t = 0 as usize;
    //     unsafe {
    //         var_int_t = sys::proc_name(pid, name_buf.as_mut_ptr() as *mut libc::c_void, 16) as usize;
    //         name_buf.set_len(var_int_t);
    //     };
    //     name = String::from_utf8(name_buf).unwrap_or("".to_string());

    name
}

fn get_proc_path(pid: i32) -> String {
    let mut pathbuf: Vec<u8> = Vec::with_capacity(4 * 1024); // 4 * MAXPATHLEN
    #[allow(unused)]
    let mut ret: i32 = 0;
    let mut out = String::new();

    unsafe {
        ret = sys::proc_pidpath(
            pid,
            pathbuf.as_mut_ptr() as *mut libc::c_void,
            pathbuf.capacity() as u32,
        );
    };

    if ret > 0 {
        unsafe {
            pathbuf.set_len(ret as usize);
        }
        out = String::from_utf8(pathbuf)
            .unwrap_or("An error occurred while retrieving process path".to_string())
    }
    out
}

pub fn macos_cwd(pid: libc::c_int) -> Result<PathBuf, Error> {
    let mut info = MaybeUninit::<sys::proc_vnodepathinfo>::uninit();
    let info_ptr = info.as_mut_ptr() as *mut libc::c_void;
    let size = std::mem::size_of::<sys::proc_vnodepathinfo>() as c_int;

    let c_str = unsafe {
        let pidinfo_size =
            sys::proc_pidinfo(pid, sys::PROC_PIDVNODEPATHINFO, 0, info_ptr, size);
        match pidinfo_size {
            c if c < 0 => return Err(io::Error::last_os_error().into()),
            s if s != size => return Err(Error::InvalidSize),
            _ => CStr::from_ptr(info.assume_init().pvi_cdir.vip_path.as_ptr()),
        }
    };

    Ok(CString::from(c_str).into_string().map(PathBuf::from)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{env, process};

    #[test]
    fn cwd_matches_current_dir() {
        assert_eq!(
            macos_cwd(process::id() as i32).ok(),
            env::current_dir().ok()
        );
    }
}
