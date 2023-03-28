use crate::term::Term;
use crate::Event;

use crate::event::control::C0;
use crate::event::sync::FairMutex;
use colors::{AnsiColor, NamedColor};
use crosswords::{attr::*, Crosswords};
use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::error::Error;
use std::fmt::Write;
use std::fmt::{self, Debug, Formatter};
use std::fs::File;
use std::io::{self, ErrorKind};
use std::io::{BufReader, Read};
use std::marker::Send;
use std::rc::Rc;
use crate::scheduler::Scheduler;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;
use teletypewriter::Pty;
use winit::event::Event::WindowEvent;
use winit::event_loop::{
    ControlFlow, DeviceEventFilter, EventLoop, EventLoopProxy, EventLoopWindowTarget,
};
use winit::platform::run_return::EventLoopExtRunReturn;
// https://vt100.net/emu/dec_ansi_parser
// use mio::net::UnixStream;
use mio::{self, Events};
use mio_extras::channel::{self, Receiver, Sender};
use vte::{Params, ParamsIter, Parser};

pub type Square = crosswords::square::Square;
pub type Row = crosswords::row::Row<Square>;
pub type VisibleRows = Arc<Mutex<Vec<Row>>>;
pub type WindowTitle = Arc<Mutex<String>>;
#[derive(Copy, Clone, Debug)]
pub struct WindowSize {
    pub num_lines: u16,
    pub num_cols: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

pub struct Sequencer {
    // term: Term,
    config: Rc<config::Config>,
}

impl Sequencer {
    /// Create a new event processor.
    ///
    /// Takes a writer which is expected to be hooked up to the write end of a PTY.
    pub fn new(config: config::Config) -> Sequencer {
        Sequencer {
            config: Rc::new(config),
        }
    }

    pub async fn run(
        &mut self,
        mut event_loop: EventLoop<Event>,
    ) -> Result<(), Box<dyn Error>> {
        let proxy = event_loop.create_proxy();
        let mut scheduler = Scheduler::new(proxy.clone());
        let window_builder = crate::window::create_window_builder(
            "Rio",
            (self.config.width, self.config.height),
        );
        let winit_window = window_builder.build(&event_loop).unwrap();
        let mut term = Term::new(&winit_window, &self.config).await?;

        event_loop.set_device_event_filter(DeviceEventFilter::Always);
        event_loop.run_return(move |event, _, control_flow| {
            // if Self::skip_event(&event) {
            //     return;
            // }

            match event {
                winit::event::Event::Resumed => {
                    term.configure();
                }

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::CloseRequested,
                    ..
                } => *control_flow = winit::event_loop::ControlFlow::Exit,

                // winit::event::Event::WindowEvent {
                //     event: winit::event::WindowEvent::ModifiersChanged(modifiers),
                //     ..
                // } => input_stream.set_modifiers(modifiers),

                // event::Event::WindowEvent {
                //     event: event::WindowEvent::MouseWheel { delta, .. },
                //     ..
                // } => {
                //     let mut scroll_y: f64 = 0.0;
                //     match delta {
                //         winit::event::MouseScrollDelta::LineDelta(_x, _y) => {
                //             // scroll_y = y;
                //         }

                //         winit::event::MouseScrollDelta::PixelDelta(pixel_delta) => {
                //             scroll_y = pixel_delta.y;
                //         }
                //     }

                //     // hacky
                //     if scroll_y < 0.0 {
                //         rio.set_text_scroll(-3.0_f32);
                //         // winit_window.request_redraw();
                //     }
                //     if scroll_y > 0.0 {
                //         rio.set_text_scroll(3.0_f32);
                //     }
                // }
                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::ReceivedCharacter(character),
                    ..
                } => {
                    // println!("character: {:?}", character);
                    // input_stream.input_character(character, &mut rio.write_process);
                }

                winit::event::Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::KeyboardInput {
                            input:
                                winit::event::KeyboardInput {
                                    // semantic meaning of the key
                                    virtual_keycode,
                                    // physical key pressed
                                    scancode,
                                    state,
                                    // modifiers,
                                    ..
                                },
                            ..
                        },
                    ..
                } => match state {
                    winit::event::ElementState::Pressed => {
                        // println!("{:?} {:?}", scancode, Some(virtual_keycode));
                        // input_stream.keydown(
                        //     scancode,
                        //     virtual_keycode,
                        //     &mut rio.write_process,
                        // );
                        winit_window.request_redraw();
                    }

                    winit::event::ElementState::Released => {
                        winit_window.request_redraw();
                    }
                },

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Focused(focused),
                    ..
                } => {
                    // is_focused = focused;
                }

                winit::event::Event::WindowEvent {
                    event: winit::event::WindowEvent::Resized(new_size),
                    ..
                } => {
                    // rio.set_size(new_size);
                    term.resize(new_size);
                    winit_window.request_redraw();
                }

                winit::event::Event::WindowEvent {
                    event:
                        winit::event::WindowEvent::ScaleFactorChanged {
                            new_inner_size,
                            scale_factor,
                        },
                    ..
                } => {
                    let scale_factor_f32 = scale_factor as f32;
                    // if rio.get_scale() != scale_factor_f32 {
                    // rio.set_scale(scale_factor_f32, *new_inner_size);
                    // }
                }

                winit::event::Event::MainEventsCleared { .. } => {
                    winit_window.request_redraw();
                }

                winit::event::Event::RedrawRequested { .. } => {
                    // if rio.renderer.config.advanced.disable_render_when_unfocused
                    //     && is_focused
                    // {
                    //     return;
                    // }
                    term.render(self.config.colors.background.1);
                }
                _ => {
                    // let next_frame_time =
                    // std::time::Instant::now() + std::time::Duration::from_nanos(500_000);
                    // *control_flow = winit::event_loop::ControlFlow::WaitUntil(next_frame_time);
                    // *control_flow = event_loop::ControlFlow::Wait;
                    return;
                }
            }
        });

        // if exit_code == 0 {
        Ok(())
        // } else {
        //     Err(format!("Event loop terminated with code: {}", exit_code).into())
        // }
    }
}

// pub trait Handler {
//     /// A character to be displayed.
//     fn input(&mut self, _c: char) {}
// }

// #[derive(Debug)]
// pub enum Msg {
//     /// Data that should be written to the PTY.
//     Input(Cow<'static, [u8]>),

//     /// Indicates that the `EventLoop` should shut down, as Alacritty is shutting down.
//     Shutdown,

//     /// Instruction to resize the PTY.
//     Resize(WindowSize),
// }

// const READ_BUFFER_SIZE: usize = 0x10_0000;

// struct Performer {
//     handler: Crosswords,
// }

// impl Performer {
//     fn new(columns: usize, rows: usize) -> Performer {
//         let crosswords: Crosswords = Crosswords::new(columns, rows);

//         Performer {
//             handler: crosswords,
//         }
//     }
// }

// impl vte::Perform for Performer {
//     fn print(&mut self, c: char) {
//         // println!("[print] {c:?}");
//         self.handler.input(c);
//         // let mut s = self.visible_rows.lock().unwrap();
//         // *s = self.handler.visible_rows();
//     }

//     fn execute(&mut self, byte: u8) {
//         // println!("[execute] {byte:04x}");

//         match byte {
//             C0::HT => self.handler.put_tab(1),
//             C0::BS => self.handler.backspace(),
//             C0::CR => self.handler.carriage_return(),
//             C0::LF | C0::VT | C0::FF => self.handler.linefeed(),
//             C0::BEL => self.handler.bell(),
//             C0::SUB => self.handler.substitute(),
//             // C0::SI => self.handler.set_active_charset(CharsetIndex::G0),
//             // C0::SO => self.handler.set_active_charset(CharsetIndex::G1),
//             _ => println!("[unhandled] execute byte={byte:02x}"),
//         }
//     }

//     fn hook(
//         &mut self,
//         params: &Params,
//         intermediates: &[u8],
//         ignore: bool,
//         action: char,
//     ) {
//         match (action, intermediates) {
//             ('s', [b'=']) => {
//                 // Start a synchronized update. The end is handled with a separate parser.
//                 if params.iter().next().map_or(false, |param| param[0] == 1) {
//                     // self.state.dcs = Some(Dcs::SyncStart);
//                 }
//             }
//             _ => println!(
//                 "[unhandled hook] params={:?}, ints: {:?}, ignore: {:?}, action: {:?}",
//                 params, intermediates, ignore, action
//             ),
//         }
//         // println!(
//         //     "[hook] params={params:?}, intermediates={intermediates:?}, ignore={ignore:?}, char={c:?}"
//         // );
//     }

//     fn put(&mut self, _byte: u8) {
//         // println!("[put] {byte:02x}");
//     }

//     fn unhook(&mut self) {
//         // println!("[unhook]");
//     }

//     fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
//         println!("[osc_dispatch] params={params:?} bell_terminated={bell_terminated}");

//         let _terminator = if bell_terminated { "\x07" } else { "\x1b\\" };

//         fn unhandled(params: &[&[u8]]) {
//             let mut buf = String::new();
//             for items in params {
//                 buf.push('[');
//                 for item in *items {
//                     let _ = write!(buf, "{:?}", *item as char);
//                 }
//                 buf.push_str("],");
//             }
//             println!("[unhandled osc_dispatch]: [{}] at line {}", &buf, line!());
//         }

//         if params.is_empty() || params[0].is_empty() {
//             return;
//         }

//         match params[0] {
//             // Set window title.
//             b"0" | b"2" => {
//                 if params.len() >= 2 {
//                     let title = params[1..]
//                         .iter()
//                         .flat_map(|x| std::str::from_utf8(x))
//                         .collect::<Vec<&str>>()
//                         .join(";")
//                         .trim()
//                         .to_owned();
//                     self.handler.set_title(Some(title));
//                     // println!("{:?} title", Some(title));
//                     // return;
//                 }
//                 unhandled(params);
//             }

//             // Set color index.
//             b"4" => {
//                 if params.len() <= 1 || params.len() % 2 == 0 {
//                     unhandled(params);
//                     // return;
//                 }

//                 // for chunk in params[1..].chunks(2) {
//                 // let index = match parse_number(chunk[0]) {
//                 //     Some(index) => index,
//                 //     None => {
//                 //         unhandled(params);
//                 //         continue;
//                 //     },
//                 // };

//                 // if let Some(c) = xparse_color(chunk[1]) {
//                 //     self.handler.set_color(index as usize, c);
//                 // } else if chunk[1] == b"?" {
//                 //     let prefix = format!("4;{index}");
//                 //     self.handler.dynamic_color_sequence(prefix, index as usize, terminator);
//                 // } else {
//                 //     unhandled(params);
//                 // }
//                 // }
//             }

//             b"10" | b"11" | b"12" => {
//                 if params.len() >= 2 {
//                     // if let Some(mut dynamic_code) = parse_number(params[0]) {
//                     //     for param in &params[1..] {
//                     //         // 10 is the first dynamic color, also the foreground.
//                     //         let offset = dynamic_code as usize - 10;
//                     //         let index = NamedColor::Foreground as usize + offset;

//                     //         // End of setting dynamic colors.
//                     //         if index > NamedColor::Cursor as usize {
//                     //             unhandled(params);
//                     //             break;
//                     //         }

//                     //         if let Some(color) = xparse_color(param) {
//                     //             self.handler.set_color(index, color);
//                     //         } else if param == b"?" {
//                     //             self.handler.dynamic_color_sequence(
//                     //                 dynamic_code.to_string(),
//                     //                 index,
//                     //                 terminator,
//                     //             );
//                     //         } else {
//                     //             unhandled(params);
//                     //         }
//                     //         dynamic_code += 1;
//                     //     }
//                     //     return;
//                     // }
//                 }
//                 unhandled(params);
//             }

//             b"110" => {}

//             b"111" => {}

//             b"112" => {}

//             _ => unhandled(params),
//         }
//     }

//     // Control Sequence Introducer
//     // CSI is the two-character sequence ESCape left-bracket or the 8-bit
//     // C1 code of 233 octal, 9B hex.  CSI introduces a Control Sequence, which
//     // continues until an alphabetic character is received.
//     fn csi_dispatch(
//         &mut self,
//         params: &Params,
//         intermediates: &[u8],
//         should_ignore: bool,
//         action: char,
//     ) {
//         macro_rules! csi_unhandled {
//             () => {{
//                 println!(
//                     "[csi_dispatch] params={params:#?}, intermediates={intermediates:?}, should_ignore={should_ignore:?}, action={action:?}"
//                 );
//             }};
//         }

//         if should_ignore || intermediates.len() > 1 {
//             return;
//         }

//         let mut params_iter = params.iter();
//         let handler = &mut self.handler;

//         let mut next_param_or = |default: u16| match params_iter.next() {
//             Some(&[param, ..]) if param != 0 => param,
//             _ => default,
//         };

//         match (action, intermediates) {
//             ('K', []) => handler.clear_line(next_param_or(0)),
//             ('J', []) => {}
//             ('m', []) => {
//                 if params.is_empty() {
//                     handler.terminal_attribute(Attr::Reset);
//                 } else {
//                     for attr in attrs_from_sgr_parameters(&mut params_iter) {
//                         match attr {
//                             Some(attr) => handler.terminal_attribute(attr),
//                             None => csi_unhandled!(),
//                         }
//                     }
//                 }
//             }
//             _ => {}
//         };
//     }

//     fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
//         println!(
//             "[esc_dispatch] intermediates={intermediates:?}, ignore={ignore:?}, byte={byte:02x}"
//         );
//     }
// }

// #[inline]
// fn attrs_from_sgr_parameters(params: &mut ParamsIter<'_>) -> Vec<Option<Attr>> {
//     let mut attrs = Vec::with_capacity(params.size_hint().0);

//     #[allow(clippy::while_let_on_iterator)]
//     while let Some(param) = params.next() {
//         let attr = match param {
//             [0] => Some(Attr::Reset),
//             [1] => Some(Attr::Bold),
//             [2] => Some(Attr::Dim),
//             [3] => Some(Attr::Italic),
//             [4, 0] => Some(Attr::CancelUnderline),
//             [4, 2] => Some(Attr::DoubleUnderline),
//             [4, 3] => Some(Attr::Undercurl),
//             [4, 4] => Some(Attr::DottedUnderline),
//             [4, 5] => Some(Attr::DashedUnderline),
//             [4, ..] => Some(Attr::Underline),
//             [5] => Some(Attr::BlinkSlow),
//             [6] => Some(Attr::BlinkFast),
//             [7] => Some(Attr::Reverse),
//             [8] => Some(Attr::Hidden),
//             [9] => Some(Attr::Strike),
//             [21] => Some(Attr::CancelBold),
//             [22] => Some(Attr::CancelBoldDim),
//             [23] => Some(Attr::CancelItalic),
//             [24] => Some(Attr::CancelUnderline),
//             [25] => Some(Attr::CancelBlink),
//             [27] => Some(Attr::CancelReverse),
//             [28] => Some(Attr::CancelHidden),
//             [29] => Some(Attr::CancelStrike),
//             [30] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Black))),
//             [31] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Red))),
//             [32] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Green))),
//             [33] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Yellow))),
//             [34] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Blue))),
//             [35] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Magenta))),
//             [36] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Cyan))),
//             [37] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::White))),
//             // [38] => {
//             //     // let mut iter = params.map(|param| param[0]);
//             //     // parse_sgr_color(&mut iter).map(Attr::Foreground)
//             // }
//             // [38, params @ ..] => {
//             //     // handle_colon_rgb(params).map(Attr::Foreground)
//             // }
//             [39] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::Foreground))),
//             [40] => Some(Attr::Background(AnsiColor::Named(NamedColor::Black))),
//             [41] => Some(Attr::Background(AnsiColor::Named(NamedColor::Red))),
//             [42] => Some(Attr::Background(AnsiColor::Named(NamedColor::Green))),
//             [43] => Some(Attr::Background(AnsiColor::Named(NamedColor::Yellow))),
//             [44] => Some(Attr::Background(AnsiColor::Named(NamedColor::Blue))),
//             [45] => Some(Attr::Background(AnsiColor::Named(NamedColor::Magenta))),
//             [46] => Some(Attr::Background(AnsiColor::Named(NamedColor::Cyan))),
//             [47] => Some(Attr::Background(AnsiColor::Named(NamedColor::White))),
//             // [48] => {
//             //     let mut iter = params.map(|param| param[0]);
//             //     parse_sgr_color(&mut iter).map(Attr::Background)
//             // },
//             // [48, params @ ..] => handle_colon_rgb(params).map(Attr::Background),
//             [49] => Some(Attr::Background(AnsiColor::Named(NamedColor::Background))),
//             // [58] => {
//             //     let mut iter = params.map(|param| param[0]);
//             //     parse_sgr_color(&mut iter).map(|color| Attr::UnderlineColor(Some(color)))
//             // },
//             // [58, params @ ..] => {
//             //     handle_colon_rgb(params).map(|color| Attr::UnderlineColor(Some(color)))
//             // },
//             [59] => Some(Attr::UnderlineColor(None)),
//             [90] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightBlack))),
//             [91] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightRed))),
//             [92] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightGreen))),
//             [93] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightYellow))),
//             [94] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightBlue))),
//             [95] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightMagenta))),
//             [96] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightCyan))),
//             [97] => Some(Attr::Foreground(AnsiColor::Named(NamedColor::LightWhite))),
//             [100] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightBlack))),
//             [101] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightRed))),
//             [102] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightGreen))),
//             [103] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightYellow))),
//             [104] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightBlue))),
//             [105] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightMagenta))),
//             [106] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightCyan))),
//             [107] => Some(Attr::Background(AnsiColor::Named(NamedColor::LightWhite))),
//             _ => None,
//         };
//         attrs.push(attr);
//     }

//     attrs
// }

// #[derive(Clone)]
// pub enum Event {
//     /// Grid has changed possibly requiring a mouse cursor shape change.
//     MouseCursorDirty,

//     /// Window title change.
//     Title(String),

//     /// Reset to the default window title.
//     ResetTitle,

//     /// Request to store a text string in the clipboard.
//     // ClipboardStore(ClipboardType, String),

//     /// Request to write the contents of the clipboard to the PTY.
//     ///
//     /// The attached function is a formatter which will corectly transform the clipboard content
//     /// into the expected escape sequence format.
//     // ClipboardLoad(ClipboardType, Arc<dyn Fn(&str) -> String + Sync + Send + 'static>),

//     /// Request to write the RGB value of a color to the PTY.
//     ///
//     /// The attached function is a formatter which will corectly transform the RGB color into the
//     /// expected escape sequence format.
//     // ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),

//     /// Write some text to the PTY.
//     PtyWrite(String),

//     /// Request to write the text area size.
//     TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),

//     /// Cursor blinking state has changed.
//     CursorBlinkingChange,

//     /// New terminal content available.
//     Wakeup,

//     /// Terminal bell ring.
//     Bell,

//     /// Shutdown request.
//     Exit,
// }

// impl Debug for Event {
//     fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
//         match self {
//             // Event::ClipboardStore(ty, text) => write!(f, "ClipboardStore({ty:?}, {text})"),
//             // Event::ClipboardLoad(ty, _) => write!(f, "ClipboardLoad({ty:?})"),
//             Event::TextAreaSizeRequest(_) => write!(f, "TextAreaSizeRequest"),
//             // Event::ColorRequest(index, _) => write!(f, "ColorRequest({index})"),
//             Event::PtyWrite(text) => write!(f, "PtyWrite({text})"),
//             Event::Title(title) => write!(f, "Title({title})"),
//             Event::CursorBlinkingChange => write!(f, "CursorBlinkingChange"),
//             Event::MouseCursorDirty => write!(f, "MouseCursorDirty"),
//             Event::ResetTitle => write!(f, "ResetTitle"),
//             Event::Wakeup => write!(f, "Wakeup"),
//             Event::Bell => write!(f, "Bell"),
//             Event::Exit => write!(f, "Exit"),
//         }
//     }
// }

// pub trait OnResize {
//     fn on_resize(&mut self, window_size: WindowSize);
// }

// /// Event Loop for notifying the renderer about terminal events.
// pub trait EventListener {
//     fn send_event(&self, _event: Event) {}
// }

// pub struct Notifier(pub Sender<Msg>);

// /// Byte sequences are sent to a `Notify` in response to some events.
// pub trait Notify {
//     /// Notify that an escape sequence should be written to the PTY.
//     ///
//     /// TODO this needs to be able to error somehow.
//     fn notify<B: Into<Cow<'static, [u8]>>>(&self, _: B);
// }

// impl Notify for Notifier {
//     fn notify<B>(&self, bytes: B)
//     where
//         B: Into<Cow<'static, [u8]>>,
//     {
//         let bytes = bytes.into();
//         // terminal hangs if we send 0 bytes through.
//         if bytes.len() == 0 {
//             return;
//         }

//         let _ = self.0.send(Msg::Input(bytes));
//     }
// }

// impl OnResize for Notifier {
//     fn on_resize(&mut self, window_size: WindowSize) {
//         let _ = self.0.send(Msg::Resize(window_size));
//     }
// }

// pub struct Machine<T: teletypewriter::ProcessReadWrite> {
//     // handler: Performer,
//     // parser: Parser,
//     pty: T,
//     rx: Receiver<Msg>,
//     tx: Sender<Msg>,
//     poll: mio::Poll,
//     terminal: Arc<FairMutex<Crosswords>>,
// }

// #[derive(Debug, Clone)]
// pub struct EventProxy {
//     proxy: EventLoopProxy<Event>,
//     window_id: WindowId,
// }

// impl EventProxy {
//     pub fn new(proxy: EventLoopProxy<Event>, window_id: WindowId) -> Self {
//         Self { proxy, window_id }
//     }

//     /// Send an event to the event loop.
//     pub fn send_event(&self, event: EventType) {
//         let _ = self.proxy.send_event(Event::new(event, self.window_id));
//     }
// }

// impl EventListener for EventProxy {
//     fn send_event(&self, event: TerminalEvent) {
//         let _ = self.proxy.send_event(Event::new(event.into(), self.window_id));
//     }
// }

// #[derive(Default)]
// pub struct State {
//     write_list: VecDeque<Cow<'static, [u8]>>,
//     writing: Option<Writing>,
//     parser: Parser,
// }

// impl State {
//     #[inline]
//     fn ensure_next(&mut self) {
//         if self.writing.is_none() {
//             self.goto_next();
//         }
//     }

//     #[inline]
//     fn goto_next(&mut self) {
//         self.writing = self.write_list.pop_front().map(Writing::new);
//     }

//     #[inline]
//     fn take_current(&mut self) -> Option<Writing> {
//         self.writing.take()
//     }

//     #[inline]
//     fn needs_write(&self) -> bool {
//         self.writing.is_some() || !self.write_list.is_empty()
//     }

//     #[inline]
//     fn set_current(&mut self, new: Option<Writing>) {
//         self.writing = new;
//     }
// }

// struct Writing {
//     source: Cow<'static, [u8]>,
//     written: usize,
// }

// impl Writing {
//     #[inline]
//     fn new(c: Cow<'static, [u8]>) -> Writing {
//         Writing {
//             source: c,
//             written: 0,
//         }
//     }

//     #[inline]
//     fn advance(&mut self, n: usize) {
//         self.written += n;
//     }

//     #[inline]
//     fn remaining_bytes(&self) -> &[u8] {
//         &self.source[self.written..]
//     }

//     #[inline]
//     fn finished(&self) -> bool {
//         self.written >= self.source.len()
//     }
// }

// impl<T> Machine<T>
// where
//     T: teletypewriter::ProcessReadWrite + Send + 'static + Evented,
// {
//     pub fn new(pty: T, columns: usize, rows: usize) -> Machine<T> {
//         let (tx, rx) = channel::channel();
//         Machine {
//             poll: mio::Poll::new().expect("create mio Poll"),
//             // handler,
//             tx,
//             rx,
//             pty,
//             terminal:
//             // parser }
//         }
//     }

//     pub fn channel(&self) -> Sender<Msg> {
//         self.tx.clone()
//     }

//     // pub fn process(&mut self, process: Pty) {
//     //     let reader = BufReader::new(process);
//     //     let handler = Performer::new(visible_rows_arc, columns, rows);
//     //     let parser = Parser::new();
//     //     for byte in reader.bytes() {
//     //         parser
//     //             .advance(&mut handler, *byte.as_ref().unwrap());
//     //     }
//     // }

//     #[inline]
//     fn pty_read<X>(
//         &mut self,
//         state: &mut State,
//         buf: &mut [u8],
//         mut writer: Option<&mut X>,
//     ) -> io::Result<()>
//     where
//         X: Write,
//     {
//         let mut unprocessed = 0;
//         let mut processed = 0;

//         // Reserve the next terminal lock for PTY reading.
//         let _terminal_lease = Some(self.terminal.lease());
//         let mut terminal = None;

//         loop {
//             // Read from the PTY.
//             match self.pty.reader().read(&mut buf[unprocessed..]) {
//                 // This is received on Windows/macOS when no more data is readable from the PTY.
//                 Ok(0) if unprocessed == 0 => break,
//                 Ok(got) => unprocessed += got,
//                 Err(err) => match err.kind() {
//                     ErrorKind::Interrupted | ErrorKind::WouldBlock => {
//                         // Go back to mio if we're caught up on parsing and the PTY would block.
//                         if unprocessed == 0 {
//                             break;
//                         }
//                     }
//                     _ => return Err(err),
//                 },
//             }

//             // Attempt to lock the terminal.
//             let terminal = match &mut terminal {
//                 Some(terminal) => terminal,
//                 None => terminal.insert(match self.terminal.try_lock_unfair() {
//                     // Force block if we are at the buffer size limit.
//                     None if unprocessed >= READ_BUFFER_SIZE => {
//                         self.terminal.lock_unfair()
//                     }
//                     None => continue,
//                     Some(terminal) => terminal,
//                 }),
//             };

//             // Write a copy of the bytes to the ref test file.
//             if let Some(writer) = &mut writer {
//                 writer.write_all(&buf[..unprocessed]).unwrap();
//             }

//             // Parse the incoming bytes.
//             for byte in &buf[..unprocessed] {
//                 state.parser.advance(&mut **terminal, *byte);
//             }

//             processed += unprocessed;
//             unprocessed = 0;

//             // Assure we're not blocking the terminal too long unnecessarily.
//             if processed >= MAX_LOCKED_READ {
//                 break;
//             }
//         }

//         // Queue terminal redraw unless all processed bytes were synchronized.
//         if state.parser.sync_bytes_count() < processed && processed > 0 {
//             self.event_proxy.send_event(Event::Wakeup);
//         }

//         Ok(())
//     }

//     /// Returns a `bool` indicating whether or not the event loop should continue running.
//     #[inline]
//     fn channel_event(&mut self, token: mio::Token, state: &mut State) -> bool {
//         if !self.drain_recv_channel(state) {
//             return false;
//         }

//         self.poll
//             .reregister(
//                 &self.rx,
//                 token,
//                 Ready::readable(),
//                 PollOpt::edge() | PollOpt::oneshot(),
//             )
//             .unwrap();

//         true
//     }

//     #[inline]
//     fn pty_write(&mut self, state: &mut State) -> io::Result<()> {
//         state.ensure_next();

//         'write_many: while let Some(mut current) = state.take_current() {
//             'write_one: loop {
//                 match self.pty.writer().write(current.remaining_bytes()) {
//                     Ok(0) => {
//                         state.set_current(Some(current));
//                         break 'write_many;
//                     }
//                     Ok(n) => {
//                         current.advance(n);
//                         if current.finished() {
//                             state.goto_next();
//                             break 'write_one;
//                         }
//                     }
//                     Err(err) => {
//                         state.set_current(Some(current));
//                         match err.kind() {
//                             ErrorKind::Interrupted | ErrorKind::WouldBlock => {
//                                 break 'write_many
//                             }
//                             _ => return Err(err),
//                         }
//                     }
//                 }
//             }
//         }
//     }

//     pub fn spawn(&mut self) {
//         tokio::spawn(async move {
//             let mut state = State::default();
//             let mut buf = [0u8; READ_BUFFER_SIZE];

//             let mut tokens = (0..).map(Into::into);

//             let poll_opts = PollOpt::edge() | PollOpt::oneshot();

//             let channel_token = tokens.next().unwrap();
//             self.poll
//                 .register(&self.rx, channel_token, Ready::readable(), poll_opts)
//                 .unwrap();

//             // Register TTY through EventedRW interface.
//             self.pty
//                 .register(&self.poll, &mut tokens, Ready::readable(), poll_opts)
//                 .unwrap();

//             let mut events = Events::with_capacity(1024);

//             let mut pipe = if self.ref_test {
//                 Some(
//                     File::create("./alacritty.recording")
//                         .expect("create alacritty recording"),
//                 )
//             } else {
//                 None
//             };

//             'event_loop: loop {
//                 // Wakeup the event loop when a synchronized update timeout was reached.
//                 let sync_timeout = state.parser.sync_timeout();
//                 let timeout =
//                     sync_timeout.map(|st| st.saturating_duration_since(Instant::now()));

//                 // if let Err(err) = self.poll.poll(&mut events, timeout) {
//                 //     match err.kind() {
//                 //         ErrorKind::Interrupted => continue,
//                 //         _ => panic!("EventLoop polling error: {err:?}"),
//                 //     }
//                 // }

//                 // Handle synchronized update timeout.
//                 if events.is_empty() {
//                     state.parser.stop_sync(&mut *self.terminal.lock());
//                     self.event_proxy.send_event(Event::Wakeup);
//                     continue;
//                 }

//                 for event in events.iter() {
//                     match event.token() {
//                         token if token == channel_token => {
//                             if !self.channel_event(channel_token, &mut state) {
//                                 break 'event_loop;
//                             }
//                         }

//                         token if token == self.pty.child_event_token() => {
//                             if let Some(tty::ChildEvent::Exited) =
//                                 self.pty.next_child_event()
//                             {
//                                 if self.hold {
//                                     // With hold enabled, make sure the PTY is drained.
//                                     let _ = self.pty_read(
//                                         &mut state,
//                                         &mut buf,
//                                         pipe.as_mut(),
//                                     );
//                                 } else {
//                                     // Without hold, shutdown the terminal.
//                                     self.terminal.lock().exit();
//                                 }

//                                 self.event_proxy.send_event(Event::Wakeup);
//                                 break 'event_loop;
//                             }
//                         }

//                         token
//                             if token == self.pty.read_token()
//                                 || token == self.pty.write_token() =>
//                         {
//                             #[cfg(unix)]
//                             if UnixReady::from(event.readiness()).is_hup() {
//                                 // Don't try to do I/O on a dead PTY.
//                                 continue;
//                             }

//                             if event.readiness().is_readable() {
//                                 if let Err(err) =
//                                     self.pty_read(&mut state, &mut buf, pipe.as_mut())
//                                 {
//                                     // On Linux, a `read` on the master side of a PTY can fail
//                                     // with `EIO` if the client side hangs up.  In that case,
//                                     // just loop back round for the inevitable `Exited` event.
//                                     // This sucks, but checking the process is either racy or
//                                     // blocking.
//                                     #[cfg(target_os = "linux")]
//                                     if err.raw_os_error() == Some(libc::EIO) {
//                                         continue;
//                                     }

//                                     error!(
//                                         "Error reading from PTY in event loop: {}",
//                                         err
//                                     );
//                                     break 'event_loop;
//                                 }
//                             }

//                             if event.readiness().is_writable() {
//                                 if let Err(err) = self.pty_write(&mut state) {
//                                     error!("Error writing to PTY in event loop: {}", err);
//                                     break 'event_loop;
//                                 }
//                             }
//                         }
//                         _ => (),
//                     }
//                 }

//                 // Register write interest if necessary.
//                 let mut interest = Ready::readable();
//                 if state.needs_write() {
//                     interest.insert(Ready::writable());
//                 }
//                 // Reregister with new interest.
//                 self.pty
//                     .reregister(&self.poll, interest, poll_opts)
//                     .unwrap();
//             }

//             // The evented instances are not dropped here so deregister them explicitly.
//             let _ = self.poll.deregister(&self.rx);
//             let _ = self.pty.deregister(&self.poll);

//             (self, state)

//             // let reader = BufReader::new(self.pty);
//             // let handler = Performer::new(columns, rows);
//             // let parser = Parser::new();
//             // for byte in reader.bytes() {
//             //     parser
//             //         .advance(&mut handler, *byte.as_ref().unwrap());
//             // }
//         });
//     }
// }
