mod control;

use crosswords::CrosswordsSquare;
use config::Config;
use control::C0;
use crosswords::square::Square;
use crosswords::Crosswords;
use std::io::{BufReader, Read};
use std::sync::Arc;
use std::sync::Mutex;
use tty::Process;

// https://vt100.net/emu/dec_ansi_parser
use vte::{Params, Parser, Perform};

pub trait Handler {
    /// A character to be displayed.
    fn input(&mut self, _c: char) {}
}

struct Performer<'a> {
    message: &'a Arc<Mutex<String>>,
    handler: Crosswords<Square>
}

impl<'a> Performer<'a>
{
    fn new(message: &Arc<Mutex<String>>, columns: u16, rows: u16) -> Performer {
        let crosswords: Crosswords<Square> =
        Crosswords::new(columns.into(), rows.into());

        Performer { 
            message,
            handler: crosswords
        }
    }
}

impl<'a> vte::Perform for Performer<'a>
// where
//     H: Handler + 'a,
{
    fn print(&mut self, c: char) {
        // println!("[print] {c:?}");
        self.handler.input(c);
        // println!("{:?}", self.handler.to_string());

        let mut s = self.message.lock().unwrap();
        *s = self.handler.to_string();

        // let s = &mut *self.message.lock().unwrap();
        // s.push(c);
    }

    fn execute(&mut self, byte: u8) {
        println!("[execute] {byte:04x}");

        match byte {
            C0::HT => {
                // TODO: Insert tab at cursor position
                // self.handler.put_tab(1)
                // let s = &mut *self.message.lock().unwrap();
                // s.push(' ');
            }
            C0::BS => {
                // TODO: Move back cursor
                self.handler.backspace();
                // let mut s = self.message.lock().unwrap();
                // s.pop();
                // *s = s.to_string()
            }
            // C0::CR => self.handler.carriage_return(),
            C0::LF | C0::VT | C0::FF => {
                // TODO: add new line
                // let s = &mut *self.message.lock().unwrap();
                // s.push('\n');
                self.handler.feedline();
            }
            // C0::BEL => self.handler.bell(),
            // C0::SUB => self.handler.substitute(),
            // C0::SI => self.handler.set_active_charset(CharsetIndex::G0),
            // C0::SO => self.handler.set_active_charset(CharsetIndex::G1),
            _ => println!("[unhandled] execute byte={byte:02x}"),
        }
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
        println!(
            "[hook] params={params:?}, intermediates={intermediates:?}, ignore={ignore:?}, char={c:?}"
        );
    }

    fn put(&mut self, byte: u8) {
        println!("[put] {byte:02x}");
    }

    fn unhook(&mut self) {
        println!("[unhook]");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        println!("[osc_dispatch] params={params:?} bell_terminated={bell_terminated}");
    }

    // Control Sequence Introducer
    // CSI is the two-character sequence ESCape left-bracket or the 8-bit
    // C1 code of 233 octal, 9B hex.  CSI introduces a Control Sequence, which
    // continues until an alphabetic character is received.
    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        c: char,
    ) {
        println!(
            "[csi_dispatch] params={params:#?}, intermediates={intermediates:?}, ignore={ignore:?}, char={c:?}"
        );

        // TODO: Implement params

        // if c == 'J' && params.len() > 1 {
            // let mut s = self.message.lock().unwrap();
            // *s = String::from("");
        // }

        // if c == 'K' {
        //     let mut s = self.message.lock().unwrap();
        //     s.pop();
        //     *s = s.to_string();
        // }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        println!(
            "[esc_dispatch] intermediates={intermediates:?}, ignore={ignore:?}, byte={byte:02x}"
        );
    }
}

// ■ ~ ▲
pub fn process(process: Process, arc_m: &Arc<Mutex<String>>, config: Config) {
    let reader = BufReader::new(process);

    let mut handler = Performer::new(arc_m, config.columns, config.rows);
    let mut parser = Parser::new();
    for byte in reader.bytes() {
        parser.advance(&mut handler, *byte.as_ref().unwrap());

        // let bs = crate::shared::utils::convert_to_utf8_string(byte.unwrap());
        // let mut a = arc_m.lock().unwrap();
        // *a = format!("{}{}", *a, bs);
    }
}
