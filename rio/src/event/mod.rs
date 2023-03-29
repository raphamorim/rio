pub mod sync;
use mio_extras::channel::Sender;
use std::borrow::Cow;
use std::sync::Arc;
use std::fmt::Debug;
use std::fmt::Formatter;

#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    /// Indicates that the `EventLoop` should shut down, as Alacritty is shutting down.
    Shutdown,

    /// Instruction to resize the PTY.
    Resize(WindowSize),
}

#[derive(Copy, Clone, Debug)]
pub struct WindowSize {
    pub num_lines: u16,
    pub num_cols: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

#[derive(Clone)]
pub enum Event {
    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    // ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// The attached function is a formatter which will corectly transform the clipboard content
    /// into the expected escape sequence format.
    // ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// The attached function is a formatter which will corectly transform the RGB color into the
    /// expected escape sequence format.
    // ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),

    /// Write some text to the PTY.
    PtyWrite(String),

    /// Request to write the text area size.
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    /// New terminal content available.
    Wakeup,

    /// Terminal bell ring.
    Bell,

    /// Shutdown request.
    Exit,
}

impl Debug for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            // Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({ty:?}, {text})"),
            // Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({ty:?})"),
            Event::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
            // Event::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
            Event::PtyWrite(text) => write!(f, "PtyWrite({text})"),
            Event::Title(title) => write!(f, "Title({title})"),
            Event::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            Event::ResetTitle => write!(f, "ResetTitle"),
            Event::Wakeup => write!(f, "Wakeup"),
            Event::Bell => write!(f, "Bell"),
            Event::Exit => write!(f, "Exit"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum EventType {
    ScaleFactorChanged(f64, (u32, u32)),
    Terminal(Event),
    // ConfigReload(PathBuf),
    // Message(Message),
    // Scroll(Scroll),
    BlinkCursor,
    BlinkCursorTimeout,
    SearchNext,
    Frame,
}

impl From<Event> for EventType {
    fn from(event: Event) -> Self {
        Self::Terminal(event)
    }
}

#[derive(Debug, Clone)]
pub struct EventP {
    /// Event payload.
    payload: EventType,
}

impl EventP {
    pub fn new(payload: EventType) -> Self {
        Self { payload }
    }
}

impl From<Event> for winit::event::Event<'_, Event> {
    fn from(event: Event) -> Self {
        winit::event::Event::UserEvent(event)
    }
}


pub trait OnResize {
    fn on_resize(&mut self, window_size: WindowSize);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn send_event(&self, _event: crate::event::Event) {}
}

pub struct Notifier(pub Sender<Msg>);

/// Byte sequences are sent to a `Notify` in response to some events.
pub trait Notify {
    /// Notify that an escape sequence should be written to the PTY.
    ///
    /// TODO this needs to be able to error somehow.
    fn notify<B: Into<Cow<'static, [u8]>>>(&self, _: B);
}

impl Notify for Notifier {
    fn notify<B>(&self, bytes: B)
    where
        B: Into<Cow<'static, [u8]>>,
    {
        let bytes = bytes.into();
        // terminal hangs if we send 0 bytes through.
        if bytes.len() == 0 {
            return;
        }

        let _ = self.0.send(Msg::Input(bytes));
    }
}

impl OnResize for Notifier {
    fn on_resize(&mut self, window_size: WindowSize) {
        let _ = self.0.send(Msg::Resize(window_size));
    }
}

pub struct VoidListener;

impl EventListener for VoidListener {}