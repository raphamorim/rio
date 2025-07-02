mod child;
mod conpty;
mod pipes;
mod spsc;

use std::ffi::OsStr;
use std::io::{self};
use std::iter::once;
use std::mem::{size_of, MaybeUninit};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::ptr::null_mut;
use std::sync::mpsc::TryRecvError;

use crate::windows::child::ChildExitWatcher;
use crate::{ChildEvent, EventedPty, ProcessReadWrite, Winsize, WinsizeBuilder};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, MAX_PATH, FILETIME};
use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows_sys::Win32::System::Memory::VirtualQueryEx;
use windows_sys::Win32::System::ProcessStatus::GetProcessImageFileNameW;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW,
    PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, GetProcessTimes,
};
// Try alternative import path for ToolHelp
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
    TH32CS_SNAPPROCESS,
};

// Import ntapi for process parameter structures
use ntapi::ntpebteb::PEB;
use ntapi::ntpsapi::{
    NtQueryInformationProcess, ProcessBasicInformation, ProcessWow64Information,
    PROCESS_BASIC_INFORMATION,
};
use ntapi::ntrtl::RTL_USER_PROCESS_PARAMETERS;
use ntapi::ntwow64::{PEB32, RTL_USER_PROCESS_PARAMETERS32};

use conpty::Conpty as Backend;
use pipes::{EventedAnonRead as ReadPipe, EventedAnonWrite as WritePipe};

pub struct Pty {
    // Backend is required to be the first field, to ensure correct drop order. Dropping
    // `conout` before `backend` will cause a deadlock (with Conpty).
    backend: Backend,
    conout: ReadPipe,
    conin: WritePipe,
    read_token: corcovado::Token,
    write_token: corcovado::Token,
    child_event_token: corcovado::Token,
    child_watcher: ChildExitWatcher,
}

// Creates conpty instead of pty
// Windows Pseudo Console (ConPTY)
pub fn create_pty(
    shell: &str,
    args: Vec<String>,
    working_directory: &Option<String>,
    columns: u16,
    rows: u16,
) -> Result<Pty, std::io::Error> {
    let exec = if !args.is_empty() {
        let args = args.join(" ");
        &format!("{shell} {args}")
    } else {
        shell
    };
    conpty::new(exec, working_directory, columns, rows)
}

impl Pty {
    fn new(
        backend: impl Into<Backend>,
        conout: impl Into<ReadPipe>,
        conin: impl Into<WritePipe>,
        child_watcher: ChildExitWatcher,
    ) -> Self {
        Self {
            backend: backend.into(),
            conout: conout.into(),
            conin: conin.into(),
            read_token: 0.into(),
            write_token: 0.into(),
            child_event_token: 0.into(),
            child_watcher,
        }
    }

    pub fn child_watcher(&self) -> &ChildExitWatcher {
        &self.child_watcher
    }
}

impl ProcessReadWrite for Pty {
    type Reader = ReadPipe;
    type Writer = WritePipe;

    #[inline]
    fn register(
        &mut self,
        poll: &corcovado::Poll,
        token: &mut dyn Iterator<Item = corcovado::Token>,
        interest: corcovado::Ready,
        poll_opts: corcovado::PollOpt,
    ) -> io::Result<()> {
        self.read_token = token.next().unwrap();
        self.write_token = token.next().unwrap();

        if interest.is_readable() {
            poll.register(
                &self.conout,
                self.read_token,
                corcovado::Ready::readable(),
                poll_opts,
            )?
        } else {
            poll.register(
                &self.conout,
                self.read_token,
                corcovado::Ready::empty(),
                poll_opts,
            )?
        }
        if interest.is_writable() {
            poll.register(
                &self.conin,
                self.write_token,
                corcovado::Ready::writable(),
                poll_opts,
            )?
        } else {
            poll.register(
                &self.conin,
                self.write_token,
                corcovado::Ready::empty(),
                poll_opts,
            )?
        }

        self.child_event_token = token.next().unwrap();
        poll.register(
            self.child_watcher.event_rx(),
            self.child_event_token,
            corcovado::Ready::readable(),
            poll_opts,
        )?;

        Ok(())
    }

    #[inline]
    fn reregister(
        &mut self,
        poll: &corcovado::Poll,
        interest: corcovado::Ready,
        poll_opts: corcovado::PollOpt,
    ) -> io::Result<()> {
        if interest.is_readable() {
            poll.reregister(
                &self.conout,
                self.read_token,
                corcovado::Ready::readable(),
                poll_opts,
            )?;
        } else {
            poll.reregister(
                &self.conout,
                self.read_token,
                corcovado::Ready::empty(),
                poll_opts,
            )?;
        }
        if interest.is_writable() {
            poll.reregister(
                &self.conin,
                self.write_token,
                corcovado::Ready::writable(),
                poll_opts,
            )?;
        } else {
            poll.reregister(
                &self.conin,
                self.write_token,
                corcovado::Ready::empty(),
                poll_opts,
            )?;
        }

        poll.reregister(
            self.child_watcher.event_rx(),
            self.child_event_token,
            corcovado::Ready::readable(),
            poll_opts,
        )?;

        Ok(())
    }

    #[inline]
    fn deregister(&mut self, poll: &corcovado::Poll) -> io::Result<()> {
        poll.deregister(&self.conout)?;
        poll.deregister(&self.conin)?;
        poll.deregister(self.child_watcher.event_rx())?;
        Ok(())
    }

    #[inline]
    fn reader(&mut self) -> &mut Self::Reader {
        &mut self.conout
    }

    #[inline]
    fn read_token(&self) -> corcovado::Token {
        self.read_token
    }

    #[inline]
    fn writer(&mut self) -> &mut Self::Writer {
        &mut self.conin
    }

    #[inline]
    fn write_token(&self) -> corcovado::Token {
        self.write_token
    }

    #[inline]
    fn set_winsize(
        &mut self,
        winsize_builder: WinsizeBuilder,
    ) -> Result<(), std::io::Error> {
        let winsize: Winsize = winsize_builder.build();
        self.backend.on_resize(winsize);
        Ok(())
    }
}

impl EventedPty for Pty {
    fn child_event_token(&self) -> corcovado::Token {
        self.child_event_token
    }

    fn next_child_event(&mut self) -> Option<ChildEvent> {
        match self.child_watcher.event_rx().try_recv() {
            Ok(ev) => Some(ev),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(ChildEvent::Exited),
        }
    }
}

fn cmdline(shell: &str) -> String {
    if !shell.is_empty() {
        return shell.to_string();
    }

    once("powershell")
        // .chain(shell.args().iter().map(|a| a.as_ref()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts the string slice into a Windows-standard representation for "W"-
/// suffixed function variants, which accept UTF-16 encoded string values.
pub fn win32_string<S: AsRef<OsStr> + ?Sized>(value: &S) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(once(0)).collect()
}

pub fn spawn_daemon<I, S>(program: &str, args: I) -> io::Result<()>
where
    I: IntoIterator<Item = S> + Copy,
    S: AsRef<OsStr>,
{
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
        .spawn()
        .map(|_| ())
}

/// Get working directory of the foreground process.
///
/// On Windows, this reads the actual working directory from the process's
/// RTL_USER_PROCESS_PARAMETERS structure in the PEB (Process Environment Block).
/// This implementation builds a process tree and finds the most appropriate
/// process to get the working directory from, similar to WezTerm's approach.
pub fn foreground_process_path(
    shell_pid: u32,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Build a process tree starting from the shell
    if let Some(process_info) = build_process_tree(shell_pid) {
        // Find the best working directory from the process tree
        if let Some(best_cwd) = find_best_working_directory(&process_info) {
            return Ok(best_cwd);
        }
    }
    
    // Fallback to just the shell process
    get_process_cwd(shell_pid)
}

/// Process information similar to WezTerm's LocalProcessInfo
#[derive(Debug)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    name: String,
    cwd: Option<PathBuf>,
    start_time: Option<u64>,
    children: std::collections::HashMap<u32, ProcessInfo>,
}

/// Build a process tree starting from the given root PID
fn build_process_tree(root_pid: u32) -> Option<ProcessInfo> {
    let procs = get_all_processes()?;
    
    // Find the root process
    let root_entry = procs.iter().find(|p| p.th32ProcessID == root_pid)?;
    
    Some(build_process_info(root_entry, &procs))
}

/// Get all processes using ToolHelp32 snapshot
fn get_all_processes() -> Option<Vec<PROCESSENTRY32W>> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == std::ptr::null_mut() {
            return None;
        }
        
        let _guard = HandleGuard(snapshot);
        
        let mut processes = Vec::new();
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        
        if Process32FirstW(snapshot, &mut entry) == 0 {
            return None;
        }
        
        loop {
            processes.push(entry);
            
            if Process32NextW(snapshot, &mut entry) == 0 {
                break;
            }
        }
        
        Some(processes)
    }
}

/// Build ProcessInfo recursively, similar to WezTerm's build_proc
fn build_process_info(entry: &PROCESSENTRY32W, all_procs: &[PROCESSENTRY32W]) -> ProcessInfo {
    let mut children = std::collections::HashMap::new();
    
    // Find all children of this process
    for child_entry in all_procs {
        if child_entry.th32ParentProcessID == entry.th32ProcessID 
            && child_entry.th32ProcessID != entry.th32ProcessID {
            children.insert(
                child_entry.th32ProcessID,
                build_process_info(child_entry, all_procs)
            );
        }
    }
    
    // Get process name
    let name_end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
    let name = String::from_utf16_lossy(&entry.szExeFile[..name_end]);
    
    // Try to get working directory and start time
    let (cwd, start_time) = if let Ok(handle) = open_process_handle(entry.th32ProcessID) {
        let cwd = get_process_cwd(entry.th32ProcessID).ok();
        let start_time = get_process_start_time(handle);
        (cwd, start_time)
    } else {
        (None, None)
    };
    
    ProcessInfo {
        pid: entry.th32ProcessID,
        ppid: entry.th32ParentProcessID,
        name,
        cwd,
        start_time,
        children,
    }
}

/// Open a process handle for querying information
fn open_process_handle(pid: u32) -> Result<HANDLE, Box<dyn std::error::Error>> {
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            0,
            pid,
        );
        
        if handle == std::ptr::null_mut() {
            Err(format!("Failed to open process {}", pid).into())
        } else {
            Ok(handle)
        }
    }
}

/// Get process start time (similar to WezTerm's approach)
fn get_process_start_time(handle: HANDLE) -> Option<u64> {
    use windows_sys::Win32::System::Threading::GetProcessTimes;
    use windows_sys::Win32::Foundation::FILETIME;
    
    unsafe {
        let mut creation_time = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut exit_time = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut kernel_time = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let mut user_time = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        
        if GetProcessTimes(
            handle,
            &mut creation_time,
            &mut exit_time,
            &mut kernel_time,
            &mut user_time,
        ) != 0 {
            Some((creation_time.dwHighDateTime as u64) << 32 | creation_time.dwLowDateTime as u64)
        } else {
            None
        }
    }
}

/// Find the best working directory from a process tree
/// This implements WezTerm-like logic to find the most appropriate process
fn find_best_working_directory(process_info: &ProcessInfo) -> Option<PathBuf> {
    // First, try to find the most recently started child process with a working directory
    let mut best_candidate: Option<(&ProcessInfo, u64)> = None;
    
    // Recursively search through all children
    find_best_candidate_recursive(process_info, &mut best_candidate);
    
    // If we found a good candidate, use its working directory
    if let Some((candidate, _)) = best_candidate {
        if let Some(ref cwd) = candidate.cwd {
            return Some(cwd.clone());
        }
    }
    
    // Fallback to the root process's working directory
    process_info.cwd.clone()
}

/// Recursively find the best candidate process
fn find_best_candidate_recursive<'a>(
    process_info: &'a ProcessInfo,
    best_candidate: &mut Option<(&'a ProcessInfo, u64)>
) {
    // Check if this process is a good candidate
    if let (Some(ref _cwd), Some(start_time)) = (&process_info.cwd, process_info.start_time) {
        let priority = get_process_priority(&process_info.name);
        
        // Combine start time with priority (more recent + higher priority = better)
        let score = start_time + (priority as u64 * 1000000); // Priority boost
        
        match best_candidate {
            None => {
                *best_candidate = Some((process_info, score));
            }
            Some((_, current_score)) => {
                if score > *current_score {
                    *best_candidate = Some((process_info, score));
                }
            }
        }
    }
    
    // Recursively check all children
    for child in process_info.children.values() {
        find_best_candidate_recursive(child, best_candidate);
    }
}

/// Get priority for different process types (higher number = higher priority)
fn get_process_priority(process_name: &str) -> u32 {
    let name_lower = process_name.to_lowercase();
    match name_lower.as_str() {
        // Interactive shells and editors get highest priority
        "cmd.exe" | "powershell.exe" | "pwsh.exe" | "bash.exe" | "zsh.exe" | "fish.exe" => 100,
        "notepad.exe" | "code.exe" | "vim.exe" | "nvim.exe" | "nano.exe" => 90,
        // Development tools
        "git.exe" | "node.exe" | "python.exe" | "cargo.exe" | "rustc.exe" => 80,
        // Other command line tools
        name if name.ends_with(".exe") => 50,
        // System processes get lowest priority
        _ => 10,
    }
}

/// Get the current working directory for a specific process
fn get_process_cwd(pid: u32) -> Result<PathBuf, Box<dyn std::error::Error>> {
    unsafe {
        // Open the process with query information access
        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            0, // bInheritHandle = FALSE
            pid,
        );

        if process_handle == std::ptr::null_mut() {
            return Err(format!(
                "Failed to open process {}: {}",
                pid,
                io::Error::last_os_error()
            )
            .into());
        }

        // Ensure we close the handle when we're done
        let _handle_guard = HandleGuard(process_handle);

        // Check if this is a 32-bit process running under WOW64
        let mut wow64_info = MaybeUninit::<*const std::ffi::c_void>::uninit();
        if NtQueryInformationProcess(
            process_handle as _,
            ProcessWow64Information,
            wow64_info.as_mut_ptr() as _,
            size_of::<*const std::ffi::c_void>() as u32,
            null_mut(),
        ) != 0
        {
            return Err("Failed to query WOW64 information".into());
        }

        let wow64_info = wow64_info.assume_init();

        if wow64_info.is_null() {
            // 64-bit process
            get_cwd_64bit(process_handle)
        } else {
            // 32-bit process running under WOW64
            get_cwd_32bit(process_handle, wow64_info)
        }
    }
}

/// Get working directory from a 64-bit process
unsafe fn get_cwd_64bit(
    process_handle: HANDLE,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Get basic process information
    let mut basic_info = MaybeUninit::<PROCESS_BASIC_INFORMATION>::uninit();
    if NtQueryInformationProcess(
        process_handle as _,
        ProcessBasicInformation,
        basic_info.as_mut_ptr() as _,
        size_of::<PROCESS_BASIC_INFORMATION>() as u32,
        null_mut(),
    ) != 0
    {
        return Err("Failed to get basic process information".into());
    }

    let basic_info = basic_info.assume_init();

    // Read the PEB
    let mut peb = MaybeUninit::<PEB>::uninit();
    if ReadProcessMemory(
        process_handle as _,
        basic_info.PebBaseAddress as *const std::ffi::c_void,
        peb.as_mut_ptr() as _,
        size_of::<PEB>(),
        null_mut(),
    ) == 0
    {
        return Err("Failed to read PEB".into());
    }

    let peb = peb.assume_init();

    // Read the process parameters
    let mut params = MaybeUninit::<RTL_USER_PROCESS_PARAMETERS>::uninit();
    if ReadProcessMemory(
        process_handle as _,
        peb.ProcessParameters as *const std::ffi::c_void,
        params.as_mut_ptr() as _,
        size_of::<RTL_USER_PROCESS_PARAMETERS>(),
        null_mut(),
    ) == 0
    {
        return Err("Failed to read process parameters".into());
    }

    let params = params.assume_init();

    // Read the current directory string
    read_unicode_string(
        process_handle,
        params.CurrentDirectory.DosPath.Buffer,
        params.CurrentDirectory.DosPath.Length as usize,
    )
}

/// Get working directory from a 32-bit process running under WOW64
unsafe fn get_cwd_32bit(
    process_handle: HANDLE,
    peb32_addr: *const std::ffi::c_void,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Read the 32-bit PEB
    let mut peb32 = MaybeUninit::<PEB32>::uninit();
    if ReadProcessMemory(
        process_handle as _,
        peb32_addr,
        peb32.as_mut_ptr() as _,
        size_of::<PEB32>(),
        null_mut(),
    ) == 0
    {
        return Err("Failed to read PEB32".into());
    }

    let peb32 = peb32.assume_init();

    // Read the 32-bit process parameters
    let mut params = MaybeUninit::<RTL_USER_PROCESS_PARAMETERS32>::uninit();
    if ReadProcessMemory(
        process_handle as _,
        peb32.ProcessParameters as *const std::ffi::c_void,
        params.as_mut_ptr() as _,
        size_of::<RTL_USER_PROCESS_PARAMETERS32>(),
        null_mut(),
    ) == 0
    {
        return Err("Failed to read 32-bit process parameters".into());
    }

    let params = params.assume_init();

    // Read the current directory string
    read_unicode_string(
        process_handle,
        params.CurrentDirectory.DosPath.Buffer as _,
        params.CurrentDirectory.DosPath.Length as usize,
    )
}

/// Read a Unicode string from another process's memory
unsafe fn read_unicode_string(
    process_handle: HANDLE,
    buffer_ptr: *const u16,
    length: usize,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if buffer_ptr.is_null() || length == 0 {
        return Ok(PathBuf::new());
    }

    // Allocate buffer for the string (length is in bytes, we need u16 count)
    let char_count = length / 2;
    let mut buffer = vec![0u16; char_count];

    if ReadProcessMemory(
        process_handle as _,
        buffer_ptr as _,
        buffer.as_mut_ptr() as _,
        length,
        null_mut(),
    ) == 0
    {
        return Err("Failed to read Unicode string".into());
    }

    // Convert to PathBuf, handling null termination
    let end_pos = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    let os_string = std::ffi::OsString::from_wide(&buffer[..end_pos]);
    Ok(PathBuf::from(os_string))
}

/// Get the name of the foreground process.
///
/// On Windows, this gets the process name by querying the process image filename
/// and extracting just the filename without the path.
pub fn foreground_process_name(shell_pid: u32) -> String {
    unsafe {
        // Open the process with query information access
        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            0, // bInheritHandle = FALSE
            shell_pid,
        );

        if process_handle.is_null() {
            return String::new();
        }

        // Ensure we close the handle when we're done
        let _handle_guard = HandleGuard(process_handle);

        // Try to get the process image filename (just the filename, not full path)
        let mut buffer = [0u16; MAX_PATH as usize];
        let result =
            GetProcessImageFileNameW(process_handle, buffer.as_mut_ptr(), MAX_PATH);

        if result == 0 {
            return String::new();
        }

        // Convert the wide string to a String
        let exe_path = std::ffi::OsString::from_wide(&buffer[..result as usize]);
        let exe_path = PathBuf::from(exe_path);

        // Extract just the filename without extension
        exe_path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or("")
            .to_string()
    }
}

/// RAII guard for Windows HANDLE to ensure it gets closed
struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            if self.0 != std::ptr::null_mut() {
                CloseHandle(self.0);
            }
        }
    }
}
