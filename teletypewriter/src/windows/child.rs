use corcovado::channel::{channel, Receiver, Sender};
use std::ffi::c_void;
use std::io::Error;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{
    CloseHandle, BOOLEAN, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Threading::{
    GetExitCodeProcess, GetProcessId, RegisterWaitForSingleObject, UnregisterWaitEx,
    INFINITE, WT_EXECUTEINWAITTHREAD, WT_EXECUTEONLYONCE,
};

use crate::ChildEvent;

struct ChildExitSender {
    sender: Sender<ChildEvent>,
    child_handle: AtomicPtr<c_void>,
}

/// WinAPI callback to run when child process exits.
extern "system" fn child_exit_callback(ctx: *mut c_void, timed_out: BOOLEAN) {
    if timed_out != 0 {
        return;
    }

    // Borrowed: the watcher owns the sender and frees it in its Drop,
    // after waiting for this callback to return.
    let event_tx = unsafe { &*(ctx as *const ChildExitSender) };
    let mut exit_code = 0_u32;
    let status = unsafe {
        GetExitCodeProcess(
            event_tx.child_handle.load(Ordering::Relaxed) as HANDLE,
            &mut exit_code,
        )
    };
    let exit_code = (status != 0).then_some(exit_code as i32);
    let _ = event_tx.sender.send(ChildEvent::Exited(exit_code));
}

pub struct ChildExitWatcher {
    wait_handle: AtomicPtr<c_void>,
    event_rx: Receiver<ChildEvent>,
    /// The process handle, owned: closed with the watcher.
    child_handle: HANDLE,
    pid: Option<NonZeroU32>,
    /// The callback's context, owned: freed with the watcher.
    sender: *mut ChildExitSender,
}

// HANDLE is not Send, so Send is not derived automatically for ChildExitWatcher, but raw pointers
// are generally safe to send between threads as long as the type they deference to is Send, which
// c_void is. (see https://doc.rust-lang.org/nomicon/send-and-sync.html).
unsafe impl Send for ChildExitWatcher {}

impl ChildExitWatcher {
    /// Takes ownership of `child_handle`: it is closed with the watcher.
    pub fn new(child_handle: HANDLE) -> Result<ChildExitWatcher, Error> {
        let (event_tx, event_rx) = channel::<ChildEvent>();

        let mut wait_handle: HANDLE = std::ptr::null_mut();
        let sender = Box::into_raw(Box::new(ChildExitSender {
            sender: event_tx,
            child_handle: AtomicPtr::from(child_handle),
        }));

        let success = unsafe {
            RegisterWaitForSingleObject(
                &mut wait_handle,
                child_handle,
                Some(child_exit_callback),
                sender.cast(),
                INFINITE,
                WT_EXECUTEINWAITTHREAD | WT_EXECUTEONLYONCE,
            )
        };

        if success == 0 {
            let error = Error::last_os_error();
            drop(unsafe { Box::from_raw(sender) });
            Err(error)
        } else {
            let pid = unsafe { NonZeroU32::new(GetProcessId(child_handle)) };
            Ok(ChildExitWatcher {
                wait_handle: AtomicPtr::from(wait_handle),
                event_rx,
                child_handle,
                pid,
                sender,
            })
        }
    }

    pub fn event_rx(&self) -> &Receiver<ChildEvent> {
        &self.event_rx
    }

    pub fn raw_handle(&self) -> HANDLE {
        self.child_handle
    }

    pub fn pid(&self) -> Option<NonZeroU32> {
        self.pid
    }
}

impl Drop for ChildExitWatcher {
    fn drop(&mut self) {
        unsafe {
            // Waits for a callback in flight to return, so the context
            // and the process handle outlive it. Never called from the
            // callback's own thread, which would deadlock.
            UnregisterWaitEx(
                self.wait_handle.load(Ordering::Relaxed) as HANDLE,
                INVALID_HANDLE_VALUE,
            );
            drop(Box::from_raw(self.sender));
            CloseHandle(self.child_handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;
    use std::time::Duration;

    use corcovado::{event::Events, Poll, PollOpt, Ready, Token};
    use windows_sys::Win32::Foundation::{DuplicateHandle, DUPLICATE_SAME_ACCESS};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    use super::*;

    #[test]
    pub fn event_is_emitted_when_child_exits() {
        const WAIT_TIMEOUT: Duration = Duration::from_millis(200);

        let mut child = Command::new("cmd.exe").spawn().unwrap();
        // The watcher closes the handle it is given; `child` keeps its own.
        let mut child_handle: HANDLE = std::ptr::null_mut();
        let duplicated = unsafe {
            DuplicateHandle(
                GetCurrentProcess(),
                child.as_raw_handle() as HANDLE,
                GetCurrentProcess(),
                &mut child_handle,
                0,
                0,
                DUPLICATE_SAME_ACCESS,
            )
        };
        assert_ne!(duplicated, 0);
        let child_exit_watcher = ChildExitWatcher::new(child_handle).unwrap();

        let mut events = Events::with_capacity(1);
        let poll = Poll::new().unwrap();
        let child_events_token = Token::from(0usize);

        poll.register(
            child_exit_watcher.event_rx(),
            child_events_token,
            Ready::readable(),
            PollOpt::oneshot(),
        )
        .unwrap();

        child.kill().unwrap();

        // Poll for the event or fail with timeout if nothing has been sent.
        poll.poll(&mut events, Some(WAIT_TIMEOUT)).unwrap();
        assert_eq!(events.iter().next().unwrap().token(), child_events_token);
        // Verify that at least one `ChildEvent::Exited` was received.
        assert!(matches!(
            child_exit_watcher.event_rx().try_recv(),
            Ok(ChildEvent::Exited(_))
        ));
    }
}
