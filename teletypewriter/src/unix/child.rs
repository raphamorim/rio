use super::TIOCSWINSZ;
use crate::{ChildEvent, Winsize, WinsizeBuilder};
use std::io;
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Grace between SIGHUP and SIGKILL. Shell HUP traps and history
/// flushes routinely exceed 100ms; kitty and ghostty never escalate
/// at all, so err on the long side.
const HANGUP_GRACE: Duration = Duration::from_secs(1);
/// Bound on reaping after SIGKILL. A child in uninterruptible sleep
/// survives SIGKILL; an unbounded wait would hold the lifecycle mutex
/// (and any Drop running it) forever. The child stays `Running` on
/// timeout so a later poll or terminate can finish the reap.
const KILL_REAP_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Child {
    pub id: Arc<libc::c_int>,
    pub pid: Arc<libc::pid_t>,
    #[allow(dead_code)]
    ptsname: String,
    #[allow(dead_code)]
    process: Option<std::process::Child>,
    lifecycle: Arc<Mutex<ChildLifecycle>>,
}

/// A retired child has no PID that can accidentally be signaled after reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChildLifecycle {
    Running(libc::pid_t),
    /// SIGHUP was delivered (a Drop-side hangup or a started terminate),
    /// so terminate does not signal it again: HUP traps doing
    /// non-idempotent work must run once per close.
    HungUp(libc::pid_t),
    /// SIGKILL was sent but the reap timed out (uninterruptible sleep);
    /// later attempts only re-poll instead of repeating the escalation.
    Killed(libc::pid_t),
    Exited(Option<i32>),
}

impl ChildLifecycle {
    fn wait(&mut self, options: libc::c_int) -> io::Result<Self> {
        let (Self::Running(pid) | Self::HungUp(pid) | Self::Killed(pid)) = *self else {
            return Ok(*self);
        };
        loop {
            let mut status = 0;
            let result = unsafe { libc::waitpid(pid, &mut status, options) };
            if result == pid {
                *self = Self::Exited(Some(status));
                return Ok(*self);
            }
            if result == 0 {
                return Ok(*self);
            }
            let error = io::Error::last_os_error();
            match error.raw_os_error() {
                Some(libc::EINTR) => continue,
                Some(libc::ECHILD) => {
                    *self = Self::Exited(None);
                    return Ok(*self);
                }
                _ => return Err(error),
            }
        }
    }

    fn reap_within(&mut self, timeout: Duration) -> io::Result<bool> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Self::Exited(_) = self.wait(libc::WNOHANG)? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn hangup(&mut self) -> io::Result<()> {
        let Self::Running(pid) = self.wait(libc::WNOHANG)? else {
            return Ok(());
        };
        signal(pid, libc::SIGHUP)?;
        *self = Self::HungUp(pid);
        Ok(())
    }

    fn terminate(&mut self) -> io::Result<()> {
        let pid = match self.wait(libc::WNOHANG)? {
            Self::Exited(_) => return Ok(()),
            Self::Killed(_) => {
                return if self.reap_within(Duration::from_millis(100))? {
                    Ok(())
                } else {
                    Err(timed_out())
                };
            }
            Self::Running(pid) => {
                signal(pid, libc::SIGHUP)?;
                *self = Self::HungUp(pid);
                pid
            }
            Self::HungUp(pid) => pid,
        };
        if self.reap_within(HANGUP_GRACE)? {
            return Ok(());
        }
        signal(pid, libc::SIGKILL)?;
        *self = Self::Killed(pid);
        if self.reap_within(KILL_REAP_TIMEOUT)? {
            return Ok(());
        }
        Err(timed_out())
    }
}

fn timed_out() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "child not reaped after SIGKILL")
}

/// Signal the child's process group so descendants sharing it are
/// reached (forkpty makes the child a session leader). Fall back to
/// the pid when the child still shares our own group: pre-setsid, or
/// a directly spawned process, where killpg would signal us too.
fn signal(pid: libc::pid_t, signal: libc::c_int) -> io::Result<()> {
    let pgid = unsafe { libc::getpgid(pid) };
    let result = if pgid > 0 && pgid != unsafe { libc::getpgrp() } {
        unsafe { libc::killpg(pgid, signal) }
    } else {
        unsafe { libc::kill(pid, signal) }
    };
    if result == -1 {
        let error = io::Error::last_os_error();
        let tolerated = match error.raw_os_error() {
            Some(libc::ESRCH) => true,
            // BSD killpg reports EPERM when any group member cannot be
            // signaled (macOS setuid login(1) wrapper, sudo children),
            // even though the signal reached the others (ghostty#2273).
            // Failing here would skip escalation and reaping, leaking a
            // zombie. Linux only errs when nothing was signaled at all.
            Some(libc::EPERM) => cfg!(any(
                target_os = "macos",
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "netbsd",
                target_os = "dragonfly",
            )),
            _ => false,
        };
        if !tolerated {
            return Err(error);
        }
    }
    Ok(())
}

/// A cloneable handle sharing the child's lifecycle state, for owners
/// that outlive the `Pty` (which moves into the reader thread). Going
/// through the lifecycle keeps the reaped-PID guarantee: a retired
/// child is never signaled.
#[derive(Debug, Clone)]
pub struct ChildTerminator(Arc<Mutex<ChildLifecycle>>);

impl ChildTerminator {
    /// A handle with no child; every operation is a no-op.
    pub fn retired() -> Self {
        Self(Arc::new(Mutex::new(ChildLifecycle::Exited(None))))
    }

    /// Send SIGHUP without waiting or escalating. For Drop on threads
    /// that cannot block (the reader thread's shutdown escalation may
    /// never run when the process exits right after). Contention means
    /// terminate() is already escalating, so there is nothing to add;
    /// blocking here would stall the caller for the whole grace period.
    pub fn hangup(&self) -> io::Result<()> {
        use std::sync::TryLockError;
        let mut lifecycle = match self.0.try_lock() {
            Ok(lifecycle) => lifecycle,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return Ok(()),
        };
        lifecycle.hangup()
    }
}

impl Child {
    pub(super) fn new(
        fd: libc::c_int,
        pid: libc::pid_t,
        ptsname: String,
        process: Option<std::process::Child>,
    ) -> Self {
        assert!(pid > 0);
        Self {
            id: Arc::new(fd),
            pid: Arc::new(pid),
            ptsname,
            process,
            lifecycle: Arc::new(Mutex::new(ChildLifecycle::Running(pid))),
        }
    }

    pub fn terminator(&self) -> ChildTerminator {
        ChildTerminator(self.lifecycle.clone())
    }

    pub(super) fn poll_exit(&self) -> io::Result<Option<ChildEvent>> {
        match self
            .lifecycle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .wait(libc::WNOHANG)?
        {
            ChildLifecycle::Running(_)
            | ChildLifecycle::HungUp(_)
            | ChildLifecycle::Killed(_) => Ok(None),
            ChildLifecycle::Exited(status) => Ok(Some(ChildEvent::Exited(status))),
        }
    }

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
    /// If another reaper consumed the status, return an error on every call.
    /// https://linux.die.net/man/2/waitpid
    pub fn waitpid(&self) -> Result<Option<i32>, String> {
        match self.poll_exit().map_err(|error| error.to_string())? {
            None => Ok(None),
            Some(ChildEvent::Exited(Some(status))) => Ok(Some(status)),
            Some(ChildEvent::Exited(None)) => {
                Err(io::Error::from_raw_os_error(libc::ECHILD).to_string())
            }
        }
    }

    /// Hang up the child, then force termination after a short grace period,
    /// and reap it. Repeated calls preserve the original exit status.
    pub fn terminate(&self) -> io::Result<()> {
        self.lifecycle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .terminate()
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
        if let Err(error) = self.terminate() {
            tracing::warn!(%error, "failed to terminate PTY child");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unix::create_pty_with_spawn;
    use crate::EventedPty;
    use libc::waitpid;
    use std::io::Error;
    use std::io::Read;
    use std::process::{Command, Stdio};

    fn child(script: &str) -> Child {
        let mut process = Command::new("/bin/sh")
            .args(["-c", script])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        // The script announces that its signal disposition is installed.
        let mut ready = [0];
        process
            .stdout
            .take()
            .unwrap()
            .read_exact(&mut ready)
            .unwrap();
        let pid = process.id() as libc::pid_t;
        Child::new(-1, pid, String::new(), Some(process))
    }

    fn assert_reaped(pid: libc::pid_t) {
        let mut status = 0;
        assert_eq!(unsafe { waitpid(pid, &mut status, libc::WNOHANG) }, -1);
        assert_eq!(Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
    }

    #[test]
    fn natural_exit_status_survives_repeated_wait_and_termination() {
        let child = child("printf r; exit 23");
        let pid = *child.pid;
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.waitpid().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(libc::WEXITSTATUS(status), 23);
        child.terminate().unwrap();
        assert_eq!(child.waitpid().unwrap(), Some(status));
        drop(child);
        assert_reaped(pid);
    }

    #[test]
    fn ignored_hangup_is_escalated_and_reaped_even_if_public_pid_changes() {
        let mut child = child("trap '' HUP; printf r; while :; do :; done");
        let pid = *child.pid;
        child.pid = Arc::new(0);
        child.terminate().unwrap();
        let status = child.waitpid().unwrap().unwrap();
        assert!(libc::WIFSIGNALED(status));
        assert_eq!(libc::WTERMSIG(status), libc::SIGKILL);
        child.terminate().unwrap();
        drop(child);
        assert_reaped(pid);
    }

    #[test]
    fn pty_natural_exit_emits_status_once() {
        let mut pty = create_pty_with_spawn(
            Some("/bin/sh"),
            vec!["-c".into(), "exit 29".into()],
            &None,
            None,
            80,
            24,
            0,
            0,
        )
        .unwrap();
        let pid = *pty.child.pid;
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(ChildEvent::Exited(Some(status))) = pty.next_child_event() {
                break status;
            }
            assert!(
                Instant::now() < deadline,
                "PTY exit event was not delivered"
            );
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 29);
        // A subsequent SIGCHLD must not emit the cached status a second time.
        unsafe {
            libc::raise(libc::SIGCHLD);
        }
        assert!(pty.next_child_event().is_none());
        pty.shutdown().unwrap();
        assert_eq!(pty.child.waitpid().unwrap(), Some(status));
        drop(pty);
        assert_reaped(pid);
    }

    #[test]
    fn externally_reaped_pty_emits_unknown_status_once() {
        let mut pty = create_pty_with_spawn(
            Some("/bin/sh"),
            vec!["-c".into(), "exit 0".into()],
            &None,
            None,
            80,
            24,
            0,
            0,
        )
        .unwrap();
        pty.child.process.as_mut().unwrap().wait().unwrap();
        assert!(pty.child.waitpid().is_err());
        assert!(pty.child.waitpid().is_err());
        // Ensure the signal queue has an event even if the original SIGCHLD
        // arrived while the external reaper was consuming the status.
        unsafe {
            libc::raise(libc::SIGCHLD);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(event) = pty.next_child_event() {
                assert_eq!(event, ChildEvent::Exited(None));
                break;
            }
            assert!(
                Instant::now() < deadline,
                "PTY exit event was not delivered"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        unsafe {
            libc::raise(libc::SIGCHLD);
        }
        assert!(pty.next_child_event().is_none());
        pty.shutdown().unwrap();
        assert!(pty.child.waitpid().is_err());
    }

    #[test]
    fn failed_spawn_closes_pty_descriptors() {
        const ISOLATED: &str = "RIO_TEST_FAILED_PTY_SPAWN";
        if std::env::var_os(ISOLATED).is_none() {
            // Descriptor counts are process-wide, so run outside parallel tests.
            let status = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "unix::child::tests::failed_spawn_closes_pty_descriptors",
                ])
                .env(ISOLATED, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let failed_spawn = || {
            create_pty_with_spawn(
                Some("/definitely-missing-rio-test-shell"),
                vec![],
                &None,
                None,
                80,
                24,
                0,
                0,
            )
        };
        // Warm up any process-global signal machinery before taking a baseline.
        assert!(failed_spawn().is_err());
        let descriptors = || {
            (0..1024)
                .filter(|fd| unsafe { libc::fcntl(*fd, libc::F_GETFD) != -1 })
                .collect::<Vec<_>>()
        };
        let before = descriptors();
        for _ in 0..8 {
            assert!(failed_spawn().is_err());
        }
        assert_eq!(descriptors(), before);
    }

    #[test]
    fn reaped_child_never_signals_a_reused_pid() {
        let mut original = child("printf r; exit 0");
        {
            let mut lifecycle = original.lifecycle.lock().unwrap();
            lifecycle.wait(0).unwrap();
        }
        let sentinel = child("trap - HUP; printf r; while :; do :; done");
        // Simulate PID reuse deterministically after reaping the original.
        original.pid = sentinel.pid.clone();
        original.terminate().unwrap();
        drop(original);
        // A signal delivery may be asynchronous; allow it to become observable.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(sentinel.waitpid().unwrap(), None);
        sentinel.terminate().unwrap();
    }

    #[test]
    fn terminator_hangup_after_reap_is_noop() {
        let child = child("printf r; exit 0");
        let handle = child.terminator();
        child.lifecycle.lock().unwrap().wait(0).unwrap();
        handle.hangup().unwrap();
        assert!(child.waitpid().is_ok());
    }

    #[test]
    fn terminator_hangup_delivers_sighup_without_reaping() {
        let child = child("trap 'exit 17' HUP; printf r; while :; do :; done");
        child.terminator().hangup().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.waitpid().unwrap() {
                break status;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 17);
    }

    #[test]
    fn graceful_hangup_preserves_exit_status() {
        let child = child("trap 'exit 17' HUP; printf r; while :; do :; done");
        child.terminate().unwrap();
        let status = child.waitpid().unwrap().unwrap();
        assert!(libc::WIFEXITED(status));
        assert_eq!(libc::WEXITSTATUS(status), 17);
    }

    #[test]
    fn drop_reaps_running_child() {
        let child = child("trap 'exit 17' HUP; printf r; while :; do :; done");
        let pid = *child.pid;
        drop(child);
        assert_reaped(pid);
    }

    #[test]
    fn externally_reaped_child_is_retired() {
        let mut child = child("printf r; exit 0");
        child.process.as_mut().unwrap().wait().unwrap();
        assert!(child.waitpid().is_err());
        assert_eq!(
            *child.lifecycle.lock().unwrap(),
            ChildLifecycle::Exited(None)
        );
        assert!(child.waitpid().is_err());
        assert_eq!(child.poll_exit().unwrap(), Some(ChildEvent::Exited(None)));
        child.terminate().unwrap();
    }
}
