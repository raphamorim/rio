mod control;

use control::C0;
use crosswords::{attr::*, Crosswords, NamedColor};
use std::fmt::Write;
use std::io::{BufReader, Read};
use std::sync::Arc;
use std::sync::Mutex;
use teletypewriter::Process;
use vte::ParamsIter;
// https://vt100.net/emu/dec_ansi_parser
use vte::{Params, Parser};

pub type Square = crosswords::square::Square;
pub type Row = crosswords::row::Row<Square>;
pub type VisibleRows = Arc<Mutex<Vec<Row>>>;

pub trait Handler {
    /// A character to be displayed.
    fn input(&mut self, _c: char) {}
}

struct Performer {
    arc: VisibleRows,
    handler: Crosswords,
}

impl Performer {
    fn new(arc: VisibleRows, columns: usize, rows: usize) -> Performer {
        let crosswords: Crosswords = Crosswords::new(columns, rows);

        Performer {
            arc,
            handler: crosswords,
        }
    }
}

impl vte::Perform for Performer {
    fn print(&mut self, c: char) {
        // println!("[print] {c:?}");
        self.handler.input(c);

        let mut s = self.arc.lock().unwrap();
        // let visible_rows_to_string = self.handler.visible_rows_to_string();
        // let data_ptr: Arc<Mutex<Vec<&Square>>> = Arc::new(Mutex::from(self.handler.visible_rows()));

        *s = self.handler.visible_rows();

        // let s = &mut *self.arc.lock().unwrap();
        // s.push(c);
    }

    fn execute(&mut self, byte: u8) {
        // println!("[execute] {byte:04x}");

        match byte {
            C0::HT => self.handler.put_tab(1),
            C0::BS => self.handler.backspace(),
            C0::CR => self.handler.carriage_return(),
            C0::LF | C0::VT | C0::FF => self.handler.linefeed(),
            C0::BEL => self.handler.bell(),
            C0::SUB => self.handler.substitute(),
            // C0::SI => self.handler.set_active_charset(CharsetIndex::G0),
            // C0::SO => self.handler.set_active_charset(CharsetIndex::G1),
            _ => println!("[unhandled] execute byte={byte:02x}"),
        }
    }

    fn hook(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        match (action, intermediates) {
            ('s', [b'=']) => {
                // Start a synchronized update. The end is handled with a separate parser.
                if params.iter().next().map_or(false, |param| param[0] == 1) {
                    // self.state.dcs = Some(Dcs::SyncStart);
                }
            }
            _ => println!(
                "[unhandled hook] params={:?}, ints: {:?}, ignore: {:?}, action: {:?}",
                params, intermediates, ignore, action
            ),
        }
        // println!(
        //     "[hook] params={params:?}, intermediates={intermediates:?}, ignore={ignore:?}, char={c:?}"
        // );
    }

    fn put(&mut self, byte: u8) {
        // println!("[put] {byte:02x}");
    }

    fn unhook(&mut self) {
        // println!("[unhook]");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        println!("[osc_dispatch] params={params:?} bell_terminated={bell_terminated}");

        let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };

        fn unhandled(params: &[&[u8]]) {
            let mut buf = String::new();
            for items in params {
                buf.push('[');
                for item in *items {
                    let _ = write!(buf, "{:?}", *item as char);
                }
                buf.push_str("],");
            }
            println!("[unhandled osc_dispatch]: [{}] at line {}", &buf, line!());
        }

        if params.is_empty() || params[0].is_empty() {
            return;
        }

        match params[0] {
            // Set window title.
            b"0" | b"2" => {
                if params.len() >= 2 {
                    let title = params[1..]
                        .iter()
                        .flat_map(|x| std::str::from_utf8(x))
                        .collect::<Vec<&str>>()
                        .join(";")
                        .trim()
                        .to_owned();
                    // println!("{:?} title", Some(title));
                    // self.handler.set_title(Some(title));
                    return;
                }
                unhandled(params);
            }

            // Set color index.
            b"4" => {
                println!("4");
                if params.len() <= 1 || params.len() % 2 == 0 {
                    unhandled(params);
                    return;
                }

                for chunk in params[1..].chunks(2) {
                    // let index = match parse_number(chunk[0]) {
                    //     Some(index) => index,
                    //     None => {
                    //         unhandled(params);
                    //         continue;
                    //     },
                    // };

                    // if let Some(c) = xparse_color(chunk[1]) {
                    //     self.handler.set_color(index as usize, c);
                    // } else if chunk[1] == b"?" {
                    //     let prefix = format!("4;{index}");
                    //     self.handler.dynamic_color_sequence(prefix, index as usize, terminator);
                    // } else {
                    //     unhandled(params);
                    // }
                }
            }

            b"10" | b"11" | b"12" => {
                println!(">color");
                if params.len() >= 2 {
                    // if let Some(mut dynamic_code) = parse_number(params[0]) {
                    //     for param in &params[1..] {
                    //         // 10 is the first dynamic color, also the foreground.
                    //         let offset = dynamic_code as usize - 10;
                    //         let index = NamedColor::Foreground as usize + offset;

                    //         // End of setting dynamic colors.
                    //         if index > NamedColor::Cursor as usize {
                    //             unhandled(params);
                    //             break;
                    //         }

                    //         if let Some(color) = xparse_color(param) {
                    //             self.handler.set_color(index, color);
                    //         } else if param == b"?" {
                    //             self.handler.dynamic_color_sequence(
                    //                 dynamic_code.to_string(),
                    //                 index,
                    //                 terminator,
                    //             );
                    //         } else {
                    //             unhandled(params);
                    //         }
                    //         dynamic_code += 1;
                    //     }
                    //     return;
                    // }
                }
                unhandled(params);
            }

            b"110" => {
                println!("110");
            }

            b"111" => {
                println!("111");
            }

            b"112" => {
                println!("112");
            }

            _ => unhandled(params),
        }
    }

    // Control Sequence Introducer
    // CSI is the two-character sequence ESCape left-bracket or the 8-bit
    // C1 code of 233 octal, 9B hex.  CSI introduces a Control Sequence, which
    // continues until an alphabetic character is received.
    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        should_ignore: bool,
        action: char,
    ) {
        macro_rules! csi_unhandled {
            () => {{
                println!(
                    "[csi_dispatch] params={params:#?}, intermediates={intermediates:?}, should_ignore={should_ignore:?}, action={action:?}"
                );
            }};
        }

        if should_ignore || intermediates.len() > 1 {
            return;
        }

        let mut params_iter = params.iter();
        let handler = &mut self.handler;

        let mut next_param_or = |default: u16| match params_iter.next() {
            Some(&[param, ..]) if param != 0 => param,
            _ => default,
        };

        match (action, intermediates) {
            ('K', []) => handler.clear_line(next_param_or(0)),
            ('J', []) => {}
            ('m', []) => {
                if params.is_empty() {
                    handler.terminal_attribute(Attr::Reset);
                } else {
                    for attr in attrs_from_sgr_parameters(&mut params_iter) {
                        match attr {
                            Some(attr) => handler.terminal_attribute(attr),
                            None => csi_unhandled!(),
                        }
                    }
                }
            }
            _ => {}
        };
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        println!(
            "[esc_dispatch] intermediates={intermediates:?}, ignore={ignore:?}, byte={byte:02x}"
        );
    }
}

#[inline]
fn attrs_from_sgr_parameters(params: &mut ParamsIter<'_>) -> Vec<Option<Attr>> {
    let mut attrs = Vec::with_capacity(params.size_hint().0);

    while let Some(param) = params.next() {
        let attr = match param {
            [0] => Some(Attr::Reset),
            [1] => Some(Attr::Bold),
            [2] => Some(Attr::Dim),
            [3] => Some(Attr::Italic),
            [4, 0] => Some(Attr::CancelUnderline),
            [4, 2] => Some(Attr::DoubleUnderline),
            [4, 3] => Some(Attr::Undercurl),
            [4, 4] => Some(Attr::DottedUnderline),
            [4, 5] => Some(Attr::DashedUnderline),
            [4, ..] => Some(Attr::Underline),
            [5] => Some(Attr::BlinkSlow),
            [6] => Some(Attr::BlinkFast),
            [7] => Some(Attr::Reverse),
            [8] => Some(Attr::Hidden),
            [9] => Some(Attr::Strike),
            [21] => Some(Attr::CancelBold),
            [22] => Some(Attr::CancelBoldDim),
            [23] => Some(Attr::CancelItalic),
            [24] => Some(Attr::CancelUnderline),
            [25] => Some(Attr::CancelBlink),
            [27] => Some(Attr::CancelReverse),
            [28] => Some(Attr::CancelHidden),
            [29] => Some(Attr::CancelStrike),
            [30] => Some(Attr::Foreground(Color::Named(NamedColor::Black))),
            [31] => Some(Attr::Foreground(Color::Named(NamedColor::Red))),
            [32] => Some(Attr::Foreground(Color::Named(NamedColor::Green))),
            [33] => Some(Attr::Foreground(Color::Named(NamedColor::Yellow))),
            [34] => Some(Attr::Foreground(Color::Named(NamedColor::Blue))),
            [35] => Some(Attr::Foreground(Color::Named(NamedColor::Magenta))),
            [36] => Some(Attr::Foreground(Color::Named(NamedColor::Cyan))),
            [37] => Some(Attr::Foreground(Color::Named(NamedColor::White))),
            // [38] => {
            //     let mut iter = params.map(|param| param[0]);
            //     parse_sgr_color(&mut iter).map(Attr::Foreground)
            // },
            // [38, params @ ..] => handle_colon_rgb(params).map(Attr::Foreground),
            [39] => Some(Attr::Foreground(Color::Named(NamedColor::Foreground))),
            [40] => Some(Attr::Background(Color::Named(NamedColor::Black))),
            [41] => Some(Attr::Background(Color::Named(NamedColor::Red))),
            [42] => Some(Attr::Background(Color::Named(NamedColor::Green))),
            [43] => Some(Attr::Background(Color::Named(NamedColor::Yellow))),
            [44] => Some(Attr::Background(Color::Named(NamedColor::Blue))),
            [45] => Some(Attr::Background(Color::Named(NamedColor::Magenta))),
            [46] => Some(Attr::Background(Color::Named(NamedColor::Cyan))),
            [47] => Some(Attr::Background(Color::Named(NamedColor::White))),
            // [48] => {
            //     let mut iter = params.map(|param| param[0]);
            //     parse_sgr_color(&mut iter).map(Attr::Background)
            // },
            // [48, params @ ..] => handle_colon_rgb(params).map(Attr::Background),
            [49] => Some(Attr::Background(Color::Named(NamedColor::Background))),
            // [58] => {
            //     let mut iter = params.map(|param| param[0]);
            //     parse_sgr_color(&mut iter).map(|color| Attr::UnderlineColor(Some(color)))
            // },
            // [58, params @ ..] => {
            //     handle_colon_rgb(params).map(|color| Attr::UnderlineColor(Some(color)))
            // },
            [59] => Some(Attr::UnderlineColor(None)),
            [90] => Some(Attr::Foreground(Color::Named(NamedColor::BrightBlack))),
            [91] => Some(Attr::Foreground(Color::Named(NamedColor::BrightRed))),
            [92] => Some(Attr::Foreground(Color::Named(NamedColor::BrightGreen))),
            [93] => Some(Attr::Foreground(Color::Named(NamedColor::BrightYellow))),
            [94] => Some(Attr::Foreground(Color::Named(NamedColor::BrightBlue))),
            [95] => Some(Attr::Foreground(Color::Named(NamedColor::BrightMagenta))),
            [96] => Some(Attr::Foreground(Color::Named(NamedColor::BrightCyan))),
            [97] => Some(Attr::Foreground(Color::Named(NamedColor::BrightWhite))),
            [100] => Some(Attr::Background(Color::Named(NamedColor::BrightBlack))),
            [101] => Some(Attr::Background(Color::Named(NamedColor::BrightRed))),
            [102] => Some(Attr::Background(Color::Named(NamedColor::BrightGreen))),
            [103] => Some(Attr::Background(Color::Named(NamedColor::BrightYellow))),
            [104] => Some(Attr::Background(Color::Named(NamedColor::BrightBlue))),
            [105] => Some(Attr::Background(Color::Named(NamedColor::BrightMagenta))),
            [106] => Some(Attr::Background(Color::Named(NamedColor::BrightCyan))),
            [107] => Some(Attr::Background(Color::Named(NamedColor::BrightWhite))),
            _ => None,
        };
        attrs.push(attr);
    }

    attrs
}

pub fn process(process: Process, data_ptr: VisibleRows, columns: usize, rows: usize) {
    let reader = BufReader::new(process);

    let mut handler = Performer::new(data_ptr, columns, rows);
    let mut parser = Parser::new();
    for byte in reader.bytes() {
        parser.advance(&mut handler, *byte.as_ref().unwrap());
    }
}
