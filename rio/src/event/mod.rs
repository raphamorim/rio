pub mod sync;

use crate::clipboard::ClipboardType;
use crate::crosswords::grid::Scroll;
use crate::router::ErrorReport;
use rio_config::colors::ColorRgb;
use std::borrow::Cow;
use std::fmt::Debug;
use std::fmt::Formatter;
use std::sync::Arc;
use teletypewriter::WinsizeBuilder;
use winit::event_loop::EventLoopProxy;
use winit::window::WindowId;

#[derive(Debug)]
pub enum Msg {
    /// Data that should be written to the PTY.
    Input(Cow<'static, [u8]>),

    #[allow(dead_code)]
    Shutdown,

    Resize(WinsizeBuilder),
}

#[derive(Debug, Eq, PartialEq)]
pub enum ClickState {
    None,
    Click,
    DoubleClick,
    TripleClick,
}

#[derive(Clone)]
pub enum RioEvent {
    PrepareRender(u64),
    Render,
    Scroll(Scroll),
    Minimize(bool),
    Hide,
    UpdateConfig,
    CreateWindow,
    CloseWindow,
    CreateNativeTab,
    CreateConfigEditor,
    SelectNativeTabByIndex(usize),
    SelectNativeTabLast,
    SelectNativeTabNext,
    SelectNativeTabPrev,

    ReportToAssistant(ErrorReport),

    /// Grid has changed possibly requiring a mouse cursor shape change.
    MouseCursorDirty,

    /// Window title change.
    Title(String),

    /// Reset to the default window title.
    ResetTitle,

    /// Request to store a text string in the clipboard.
    ClipboardStore(ClipboardType, String),

    /// Request to write the contents of the clipboard to the PTY.
    ///
    /// The attached function is a formatter which will corectly transform the clipboard content
    /// into the expected escape sequence format.
    ClipboardLoad(
        ClipboardType,
        Arc<dyn Fn(&str) -> String + Sync + Send + 'static>,
    ),

    /// Request to write the RGB value of a color to the PTY.
    ///
    /// The attached function is a formatter which will corectly transform the RGB color into the
    /// expected escape sequence format.
    ColorRequest(
        usize,
        Arc<dyn Fn(ColorRgb) -> String + Sync + Send + 'static>,
    ),

    /// Write some text to the PTY.
    PtyWrite(String),

    /// Request to write the text area size.
    TextAreaSizeRequest(Arc<dyn Fn(WinsizeBuilder) -> String + Sync + Send + 'static>),

    /// Cursor blinking state has changed.
    CursorBlinkingChange,

    /// New terminal content available.
    Wakeup,

    /// Terminal bell ring.
    Bell,

    /// Shutdown request.
    Exit,
}

impl Debug for RioEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            RioEvent::ClipboardStore(ty, text) => {
                write!(f, "ClipboardStore({ty:?}, {text})")
            }
            RioEvent::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({ty:?})"),
            RioEvent::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
            RioEvent::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
            RioEvent::PtyWrite(text) => write!(f, "PtyWrite({text})"),
            RioEvent::Title(title) => write!(f, "Title({title})"),
            RioEvent::Minimize(cond) => write!(f, "Minimize({cond})"),
            RioEvent::Hide => write!(f, "Hide)"),
            RioEvent::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
            RioEvent::MouseCursorDirty => write!(f, "MouseCursorDirty"),
            RioEvent::ResetTitle => write!(f, "ResetTitle"),
            RioEvent::Wakeup => write!(f, "Wakeup"),
            RioEvent::PrepareRender(millis) => write!(f, "PrepareRender({millis})"),
            RioEvent::Render => write!(f, "Render"),
            RioEvent::Scroll(scroll) => write!(f, "Scroll {scroll:?}"),
            RioEvent::Bell => write!(f, "Bell"),
            RioEvent::Exit => write!(f, "Exit"),
            RioEvent::CreateWindow => write!(f, "CreateWindow"),
            RioEvent::CloseWindow => write!(f, "CloseWindow"),
            RioEvent::CreateNativeTab => write!(f, "CreateNativeTab"),
            RioEvent::SelectNativeTabByIndex(tab_index) => {
                write!(f, "SelectNativeTabByIndex({tab_index})")
            }
            RioEvent::SelectNativeTabLast => write!(f, "SelectNativeTabLast"),
            RioEvent::SelectNativeTabNext => write!(f, "SelectNativeTabNext"),
            RioEvent::SelectNativeTabPrev => write!(f, "SelectNativeTabPrev"),
            RioEvent::CreateConfigEditor => write!(f, "CreateConfigEditor"),
            RioEvent::UpdateConfig => write!(f, "ReloadConfiguration"),
            RioEvent::ReportToAssistant(error_report) => {
                write!(f, "ReportToAssistant({})", error_report.report)
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum RioEventType {
    Rio(RioEvent),
    // Message(Message),
    BlinkCursor,
    BlinkCursorTimeout,
}

impl From<RioEvent> for RioEventType {
    fn from(rio_event: RioEvent) -> Self {
        Self::Rio(rio_event)
    }
}

#[derive(Debug, Clone)]
pub struct EventP {
    /// Event payload.
    pub payload: RioEventType,
    pub window_id: WindowId,
}

impl EventP {
    pub fn new(payload: RioEventType, window_id: WindowId) -> Self {
        Self { payload, window_id }
    }
}

impl From<EventP> for winit::event::Event<EventP> {
    fn from(event: EventP) -> Self {
        winit::event::Event::UserEvent(event)
    }
}

pub trait OnResize {
    fn on_resize(&mut self, window_size: WinsizeBuilder);
}

/// Event Loop for notifying the renderer about terminal events.
pub trait EventListener {
    fn send_event(&self, _event: RioEvent, _id: WindowId) {}

    fn send_global_event(&self, _event: RioEvent) {}
}

#[derive(Clone)]
pub struct VoidListener;

impl EventListener for VoidListener {}

#[derive(Debug, Clone)]
pub struct EventProxy {
    proxy: EventLoopProxy<EventP>,
}

impl EventProxy {
    pub fn new(proxy: EventLoopProxy<EventP>) -> Self {
        Self { proxy }
    }

    #[allow(dead_code)]
    pub fn send_event(&self, event: RioEventType, id: WindowId) {
        let _ = self.proxy.send_event(EventP::new(event, id));
    }

    // pub fn send_global_event(&self, event: RioEventType) {
    //     let _ = self.proxy.send_event(EventP::new(event));
    // }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: RioEvent, id: WindowId) {
        let _ = self.proxy.send_event(EventP::new(event.into(), id));
    }

    // fn send_global_event(&self, event: RioEvent) {
    // let _ = self.proxy.send_event(EventP::new(event.into(), id));
    // }
}
