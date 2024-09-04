mod child;
mod conpty;
mod pipes;
mod spsc;

use std::ffi::OsStr;
use std::io::{self};
use std::iter::once;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::TryRecvError;

use crate::windows::child::ChildExitWatcher;
use crate::{ChildEvent, EventedPty, ProcessReadWrite, Winsize, WinsizeBuilder};
use windows_sys::Win32::System::Threading::{CREATE_NEW_PROCESS_GROUP, CREATE_NO_WINDOW};

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
