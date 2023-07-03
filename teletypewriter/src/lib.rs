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
    ws_width: libc::c_ushort,
    ws_height: libc::c_ushort,
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
    /// Indicates the child has exited.
    Exited,
}

pub trait EventedPty: ProcessReadWrite {
    fn child_event_token(&self) -> corcovado::Token;

    /// Tries to retrieve an event.
    ///
    /// Returns `Some(event)` on success, or `None` if there are no events to retrieve.
    fn next_child_event(&mut self) -> Option<ChildEvent>;
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
        let ws_width = self.width as libc::c_ushort;
        let ws_height = self.height as libc::c_ushort;

        Winsize {
            ws_row,
            ws_col,
            ws_width,
            ws_height,
        }
    }
}
