extern crate libc;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use self::unix::*;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows::*;

use std::io;

#[repr(C)]
pub struct Winsize {
    ws_row: libc::c_ushort,
    ws_col: libc::c_ushort,
    ws_xpixel: libc::c_ushort,
    ws_ypixel: libc::c_ushort,
}

pub trait ProcessReadWrite {
    type Reader: io::Read;
    type Writer: io::Write;
    fn reader(&mut self) -> &mut Self::Reader;
    fn read_token(&self) -> corcovado::Token;
    fn writer(&mut self) -> &mut Self::Writer;
    fn write_token(&self) -> corcovado::Token;
    fn set_winsize(&mut self, _: WinsizeBuilder) -> Result<(), io::Error>;

    fn register(
        &mut self,
        _: &corcovado::Poll,
        _: &mut dyn Iterator<Item = corcovado::Token>,
        _: corcovado::Ready,
        _: corcovado::PollOpt,
    ) -> io::Result<()>;
    fn reregister(
        &mut self,
        _: &corcovado::Poll,
        _: corcovado::Ready,
        _: corcovado::PollOpt,
    ) -> io::Result<()>;
    fn deregister(&mut self, _: &corcovado::Poll) -> io::Result<()>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChildEvent {
    /// Indicates the child has exited, with the raw wait status when the
    /// platform makes it available (interpret with
    /// `std::process::ExitStatus::from_raw` / `ExitStatusExt`).
    Exited(Option<i32>),
}

pub trait EventedPty: ProcessReadWrite {
    /// Optional hook for releasing child-process resources when the I/O loop stops.
    ///
    /// The default does nothing; success does not guarantee child termination.
    /// Unix terminates and reaps its child here. Windows keeps its existing
    /// cleanup on drop, when the ConPTY backend closes the pseudoconsole.
    fn shutdown(&mut self) -> std::io::Result<()> {
        Ok(())
    }

    fn child_event_token(&self) -> corcovado::Token;

    /// Whether a child of ours runs on the pty. Its exit, reported by
    /// [`Self::next_child_event`], is what ends the session; without
    /// one, the other side of the pty closing ends it.
    fn has_child(&self) -> bool {
        true
    }

    /// Stop watching the read side: once the other side of the pty is
    /// closed nothing more is read, and a closed side is reported ready
    /// on every poll. The child event stays watched. Only Unix reports
    /// a closed side, so elsewhere this does nothing.
    fn stop_reading(&mut self, _poll: &corcovado::Poll) -> io::Result<()> {
        Ok(())
    }

    /// Tries to retrieve an event.
    ///
    /// Returns `Some(event)` on success, or `None` if there are no events to retrieve.
    fn next_child_event(&mut self) -> Option<ChildEvent>;

    /// The write side of the PTY on its own, for a thread that writes
    /// while this one reads. Taken once; [`ProcessReadWrite::writer`]
    /// is not used after that.
    type WriteHalf: io::Write + corcovado::event::Evented + Send + 'static;
    fn write_half(&mut self) -> io::Result<Self::WriteHalf>;
}

#[derive(Debug, Clone)]
pub struct WinsizeBuilder {
    pub rows: u16,
    pub cols: u16,
    pub width: u16,
    pub height: u16,
}

impl WinsizeBuilder {
    fn build(&self) -> Winsize {
        let ws_row = self.rows as libc::c_ushort;
        let ws_col = self.cols as libc::c_ushort;
        let ws_xpixel = self.width as libc::c_ushort;
        let ws_ypixel = self.height as libc::c_ushort;

        Winsize {
            ws_row,
            ws_col,
            ws_xpixel,
            ws_ypixel,
        }
    }
}
