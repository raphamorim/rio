use crate::Winsize;
use std::io::{Error, Result};
use std::os::windows::io::IntoRawHandle;
use std::sync::OnceLock;
use std::{mem, ptr};
use tracing::*;

use crate::windows::pipes::{EventedAnonRead, EventedAnonWrite};

use windows_sys::core::{HRESULT, PWSTR};
use windows_sys::s;
use windows_sys::Win32::Foundation::{
    CloseHandle, FreeLibrary, E_INVALIDARG, HANDLE, S_OK,
};
use windows_sys::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LOAD_LIBRARY_SEARCH_SYSTEM32,
};

use windows_sys::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, InitializeProcThreadAttributeList,
    UpdateProcThreadAttribute, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTF_USESTDHANDLES, STARTUPINFOEXW,
    STARTUPINFOW,
};

use std::os::windows::ffi::OsStrExt;

use crate::windows::child::ChildExitWatcher;
use crate::windows::{cmdline, win32_string, Pty};

/// Load the pseudoconsole API from the `conpty.dll` Rio ships next to
/// `rio.exe`, otherwise fall back to the in-box Windows API.
///
/// Rio's Windows packages carry `conpty.dll` and `OpenConsole.exe` from
/// the Windows Terminal project (see `misc/windows/conhost/README.md`).
/// That host passes the child's output through unchanged: sixel, iTerm2
/// and kitty graphics reach Rio, and so does the cursor position a TUI
/// leaves behind. The pseudoconsole built into most Windows builds
/// rewrites that output, which is why images never rendered on stock
/// Windows (#729, #1759) and the IME candidate window sat in the wrong
/// place (#2031).
///
/// The DLL is loaded by absolute path from `rio.exe`'s own directory,
/// never by the default search order, which also walks the working
/// directory and `PATH`: when the bundled copy is missing (a build that
/// does not package it, a user who removed it) Rio uses the in-box API
/// rather than whatever `conpty.dll` happens to be on the way.
///
/// `PSEUDOCONSOLE_RESIZE_QUIRK` is a ConPTY flag absent from the public
/// SDK header, so not in `windows-sys`. Without it the in-box
/// pseudoconsole re-emits its own reflowed buffer after a resize, over
/// the grid Rio has already reflowed, so content ends up confined to a
/// stale region once the window grows (microsoft/terminal#16911). With
/// it the host leaves reflow to the terminal. The bundled host behaves
/// that way regardless and ignores the flag, as every ConPTY ignores
/// unknown flag bits; it matters for the in-box fallback only, and
/// there only on Windows builds whose host knows the flag (Windows 10
/// builds since 2020); older ones ignore the bit and keep the repaint.
const PSEUDOCONSOLE_RESIZE_QUIRK: u32 = 0x2;

type CreatePseudoConsoleFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(HPCON);

#[derive(Clone, Copy)]
struct ConptyApi {
    create: CreatePseudoConsoleFn,
    resize: ResizePseudoConsoleFn,
    close: ClosePseudoConsoleFn,
}

impl ConptyApi {
    /// The API every pseudoconsole of this process uses, resolved on the
    /// first one: the DLL is loaded once and stays loaded, instead of a
    /// load (and a reference it never gives back) per tab or split.
    fn get() -> Self {
        static API: OnceLock<ConptyApi> = OnceLock::new();
        *API.get_or_init(Self::load)
    }

    fn load() -> Self {
        match Self::load_conpty() {
            Some(conpty) => {
                info!("Using conpty.dll for pseudoconsole");
                conpty
            }
            None => {
                // Cannot load conpty.dll - use the standard Windows API.
                info!("Using Windows API for pseudoconsole");
                Self {
                    create: CreatePseudoConsole,
                    resize: ResizePseudoConsole,
                    close: ClosePseudoConsole,
                }
            }
        }
    }

    /// Try loading the API from the `conpty.dll` next to `rio.exe`, by
    /// absolute path, so copies on `PATH` or in the working directory are
    /// never picked up.
    fn load_conpty() -> Option<Self> {
        type LoadedFn = unsafe extern "system" fn() -> isize;
        let dll = std::env::current_exe().ok()?.parent()?.join("conpty.dll");
        let dll_path = win32_string(&dll);
        unsafe {
            // The DLL's own imports resolve from its directory and
            // System32 only, so neither the working directory nor PATH
            // takes part in the load.
            let hmodule = LoadLibraryExW(
                dll_path.as_ptr(),
                ptr::null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
            );
            if hmodule.is_null() {
                let error = Error::last_os_error();
                if dll.exists() {
                    warn!(
                        "conpty.dll at {} could not be loaded: {error}",
                        dll.display()
                    );
                }
                return None;
            }
            let export = |name: &str, address: Option<LoadedFn>| {
                if address.is_none() {
                    warn!(
                        "conpty.dll at {} has no {name} export, using the Windows API",
                        dll.display()
                    );
                    FreeLibrary(hmodule);
                }
                address
            };
            let create_fn = export(
                "CreatePseudoConsole",
                GetProcAddress(hmodule, s!("CreatePseudoConsole")),
            )?;
            let resize_fn = export(
                "ResizePseudoConsole",
                GetProcAddress(hmodule, s!("ResizePseudoConsole")),
            )?;
            let close_fn = export(
                "ClosePseudoConsole",
                GetProcAddress(hmodule, s!("ClosePseudoConsole")),
            )?;

            Some(Self {
                create: mem::transmute::<LoadedFn, CreatePseudoConsoleFn>(create_fn),
                resize: mem::transmute::<LoadedFn, ResizePseudoConsoleFn>(resize_fn),
                close: mem::transmute::<LoadedFn, ClosePseudoConsoleFn>(close_fn),
            })
        }
    }
}

/// The attribute list of a `STARTUPINFOEXW`, freed with
/// `DeleteProcThreadAttributeList` once it has been initialized, on
/// every path out of `new`.
struct AttributeList {
    storage: Box<[u8]>,
    initialized: bool,
}

impl AttributeList {
    fn as_mut_ptr(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        // The list is an opaque blob the system sizes for us; the
        // pointer type is a formality, so the alignment lint is moot.
        #[allow(clippy::cast_ptr_alignment)]
        {
            self.storage.as_mut_ptr() as LPPROC_THREAD_ATTRIBUTE_LIST
        }
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { DeleteProcThreadAttributeList(self.as_mut_ptr()) }
        }
    }
}

/// RAII Pseudoconsole.
pub struct Conpty {
    pub handle: HPCON,
}

impl Drop for Conpty {
    fn drop(&mut self) {
        // XXX: This will block until the conout pipe is drained. Will cause a deadlock if the
        // conout pipe has already been dropped by this point.
        //
        // See PR #3084 and https://docs.microsoft.com/en-us/windows/console/closepseudoconsole.
        unsafe { (ConptyApi::get().close)(self.handle) }
    }
}

// The ConPTY handle can be sent between threads.
unsafe impl Send for Conpty {}

/// Builds a `CREATE_UNICODE_ENVIRONMENT` block from the current process
/// environment plus `extra_env` (which overrides inherited variables of the
/// same name): NUL-terminated `KEY=VALUE` UTF-16 entries, with a trailing
/// NUL. Sorted case-insensitively by name, which the CreateProcess
/// documentation requires of Unicode environment blocks.
fn environment_block(extra_env: Vec<(String, String)>) -> Vec<u16> {
    let mut vars: Vec<(std::ffi::OsString, std::ffi::OsString)> =
        std::env::vars_os().collect();
    for (key, value) in extra_env {
        let key = std::ffi::OsString::from(key);
        vars.retain(|(existing, _)| !existing.eq_ignore_ascii_case(&key));
        vars.push((key, value.into()));
    }
    vars.sort_by(|(a, _), (b, _)| {
        a.to_string_lossy()
            .to_uppercase()
            .cmp(&b.to_string_lossy().to_uppercase())
    });

    let mut block = Vec::new();
    for (key, value) in vars {
        block.extend(key.encode_wide());
        block.push('=' as u16);
        block.extend(value.encode_wide());
        block.push(0);
    }
    block.push(0);
    block
}

pub fn new(
    shell: Option<&str>,
    working_directory: &Option<String>,
    env: Option<Vec<(String, String)>>,
    columns: u16,
    rows: u16,
) -> Result<Pty> {
    let api = ConptyApi::get();
    let mut pty_handle: HPCON = 0;

    // Passing 0 as the size parameter allows the "system default" buffer
    // size to be used. There may be small performance and memory advantages
    // to be gained by tuning this in the future, but it's likely a reasonable
    // start point.
    let (conout, conout_pty_handle) = miow::pipe::anonymous(0)?;
    let (conin_pty_handle, conin) = miow::pipe::anonymous(0)?;

    let winsize = Winsize {
        ws_row: rows as libc::c_ushort,
        ws_col: columns as libc::c_ushort,
        ws_xpixel: 0 as libc::c_ushort,
        ws_ypixel: 0 as libc::c_ushort,
    };

    // Create the pseudoconsole on the pipes. The resize quirk keeps the
    // in-box host from repainting its reflowed buffer over Rio's grid.
    // No known host rejects the flag; should one, it would say so with
    // E_INVALIDARG, and a plain pseudoconsole beats no terminal, so that
    // one failure is retried without it.
    let conin_raw = conin_pty_handle.into_raw_handle() as HANDLE;
    let conout_raw = conout_pty_handle.into_raw_handle() as HANDLE;
    let size: COORD = winsize.into();
    let mut result = unsafe {
        (api.create)(
            size,
            conin_raw,
            conout_raw,
            PSEUDOCONSOLE_RESIZE_QUIRK,
            &mut pty_handle,
        )
    };
    if result == E_INVALIDARG {
        warn!("CreatePseudoConsole rejected flags {PSEUDOCONSOLE_RESIZE_QUIRK:#x}; retrying without them");
        result = unsafe { (api.create)(size, conin_raw, conout_raw, 0, &mut pty_handle) };
    }
    // The host holds duplicates of the pipe ends it was given and
    // releases them when the pseudoconsole closes, so ours go now:
    // kept open, they would hold the output pipe open after the host
    // is gone and the reader would never see it end.
    unsafe {
        CloseHandle(conin_raw);
        CloseHandle(conout_raw);
    }
    if result != S_OK {
        return Err(Error::other(format!(
            "CreatePseudoConsole failed: {result:#x}"
        )));
    }
    // Owned from here on, so every early return below closes it.
    let conpty = Conpty { handle: pty_handle };

    let mut success;

    // Prepare child process startup info.

    let mut size: usize = 0;

    let mut startup_info_ex: STARTUPINFOEXW = unsafe { mem::zeroed() };

    startup_info_ex.StartupInfo.lpTitle = std::ptr::null_mut() as PWSTR;

    startup_info_ex.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;

    // Setting this flag but leaving all the handles as default (null) ensures the
    // PTY process does not inherit any handles from this Rio process.
    startup_info_ex.StartupInfo.dwFlags |= STARTF_USESTDHANDLES;

    // Create the appropriately sized thread attribute list.
    unsafe {
        let failure = InitializeProcThreadAttributeList(
            ptr::null_mut(),
            1,
            0,
            &mut size as *mut usize,
        ) > 0;

        // This call was expected to return false.
        if failure {
            return Err(Error::last_os_error());
        }
    }

    let mut attr_list = AttributeList {
        storage: vec![0; size].into_boxed_slice(),
        initialized: false,
    };
    startup_info_ex.lpAttributeList = attr_list.as_mut_ptr();

    unsafe {
        success = InitializeProcThreadAttributeList(
            startup_info_ex.lpAttributeList,
            1,
            0,
            &mut size as *mut usize,
        ) > 0;

        if !success {
            return Err(Error::last_os_error());
        }
    }
    attr_list.initialized = true;

    // Set thread attribute list's Pseudo Console to the specified ConPTY.
    unsafe {
        success = UpdateProcThreadAttribute(
            startup_info_ex.lpAttributeList,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
            conpty.handle as *mut std::ffi::c_void,
            mem::size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null_mut(),
        ) > 0;

        if !success {
            return Err(Error::last_os_error());
        }
    }

    let cmdline = win32_string(&cmdline(shell));
    let cwd = working_directory.as_ref().map(win32_string);
    // With no overrides, leave the environment pointer null so the child
    // inherits ours, exactly as before.
    let mut env_block = env.map(environment_block);

    let mut proc_info: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    unsafe {
        success = CreateProcessW(
            ptr::null(),
            cmdline.as_ptr() as PWSTR,
            ptr::null_mut(),
            ptr::null_mut(),
            false as i32,
            match env_block {
                Some(_) => EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                None => EXTENDED_STARTUPINFO_PRESENT,
            },
            match env_block.as_mut() {
                Some(block) => block.as_mut_ptr() as *mut std::ffi::c_void,
                None => ptr::null_mut(),
            },
            cwd.as_ref().map_or_else(ptr::null, |s| s.as_ptr()),
            &mut startup_info_ex.StartupInfo as *mut STARTUPINFOW,
            &mut proc_info as *mut PROCESS_INFORMATION,
        ) > 0;

        if !success {
            return Err(Error::last_os_error());
        }
        // The exit watcher takes the process handle; the thread handle
        // has no further use.
        CloseHandle(proc_info.hThread);
    }

    // Before the pipe ends are wrapped: a failure here must drop the
    // pseudoconsole while the raw read end is still open, the order
    // `Pty` keeps as well, and must not leak the process handle.
    let child_watcher =
        ChildExitWatcher::new(proc_info.hProcess).inspect_err(|_| unsafe {
            CloseHandle(proc_info.hProcess);
        })?;

    let conin = EventedAnonWrite::new(conin);
    let conout = EventedAnonRead::new(conout);

    Ok(Pty::new(conpty, conout, conin, child_watcher))
}

impl Conpty {
    pub fn on_resize(&mut self, window_size: Winsize) {
        // A collapsed window reports no columns or rows; the host would
        // refuse that, so keep its last size instead.
        if window_size.ws_col == 0 || window_size.ws_row == 0 {
            return;
        }
        let result =
            unsafe { (ConptyApi::get().resize)(self.handle, window_size.into()) };
        if result != S_OK {
            warn!("ResizePseudoConsole failed: {result:#x}");
        }
    }
}

impl From<Winsize> for COORD {
    fn from(window_size: Winsize) -> Self {
        let lines = window_size.ws_row;
        let columns = window_size.ws_col;
        COORD {
            X: columns as i16,
            Y: lines as i16,
        }
    }
}
