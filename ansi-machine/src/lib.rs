mod control;

use std::io::{BufReader, Read};
use std::sync::Arc;
use std::sync::Mutex;
use control::C0;
use crosswords::Crosswords;
use tty::Process;
// https://vt100.net/emu/dec_ansi_parser
use vte::{Params, Parser};

pub trait Handler {
    /// A character to be displayed.
    fn input(&mut self, _c: char) {}
}

struct Performer<'a> {
    message: &'a Arc<Mutex<String>>,
    handler: Crosswords,
}

impl<'a> Performer<'a> {
    fn new(message: &Arc<Mutex<String>>, columns: usize, rows: usize) -> Performer {
        let crosswords: Crosswords = Crosswords::new(columns, rows);

        Performer {
            message,
            handler: crosswords,
        }
    }
}

impl<'a> vte::Perform for Performer<'a>
{
    fn print(&mut self, c: char) {
        // println!("[print] {c:?}");
        self.handler.input(c);

        let mut s = self.message.lock().unwrap();
        *s = self.handler.visible_rows_to_string();

        // let s = &mut *self.message.lock().unwrap();
        // s.push(c);
    }

    fn execute(&mut self, byte: u8) {
        println!("[execute] {byte:04x}");

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
        should_ignore: bool,
        action: char,
    ) {
        println!(
            "[csi_dispatch] params={params:#?}, intermediates={intermediates:?}, should_ignore={should_ignore:?}, action={action:?}"
        );

        // TODO: Implement params

        // if c == 'J' && params.len() > 1 {
        // let mut s = self.message.lock().unwrap();
        // *s = String::from("");
        // }

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
            _ => {

            }
        };

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

pub fn process(
    process: Process,
    arc_m: &Arc<Mutex<String>>,
    columns: usize,
    rows: usize,
) {
    let reader = BufReader::new(process);

    let mut handler = Performer::new(arc_m, columns, rows);
    let mut parser = Parser::new();
    for byte in reader.bytes() {
        parser.advance(&mut handler, *byte.as_ref().unwrap());
    }
}
