use std::io::{BufReader, Read};
use std::sync::Arc;
use std::sync::Mutex;
use tty::Process;

// https://vt100.net/emu/dec_ansi_parser
use vte::{Params, Parser, Perform};

struct Log<'a> {
    message: &'a Arc<Mutex<String>>,
}

impl Log<'_> {
    fn new(message: &Arc<Mutex<String>>) -> Log {
        Log { message }
    }
}

impl Perform for Log<'_> {
    fn print(&mut self, c: char) {
        println!("[print] {:?}", c);
        let s = &mut *self.message.lock().unwrap();
        s.push(c);
    }

    fn execute(&mut self, byte: u8) {
        println!("[execute] {:04x}", byte);
        let c = match byte {
            0x0a => "\n",
            // TODO: create a wrap/table for words
            0x09 => " ",
            _ => "",
        };

        if !c.is_empty() {
            let s = &mut *self.message.lock().unwrap();
            s.push_str(c);
        }
    }

    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, c: char) {
        println!(
            "[hook] params={:?}, intermediates={:?}, ignore={:?}, char={:?}",
            params, intermediates, ignore, c
        );
    }

    fn put(&mut self, byte: u8) {
        println!("[put] {:02x}", byte);
    }

    fn unhook(&mut self) {
        println!("[unhook]");
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        println!(
            "[osc_dispatch] params={:?} bell_terminated={}",
            params, bell_terminated
        );
    }

    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        ignore: bool,
        c: char,
    ) {
        println!(
            "[csi_dispatch] params={:#?}, intermediates={:?}, ignore={:?}, char={:?}",
            params, intermediates, ignore, c
        );

        if c == 'J' {
            let mut s = self.message.lock().unwrap();
            *s = String::from("");
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        println!(
            "[esc_dispatch] intermediates={:?}, ignore={:?}, byte={:02x}",
            intermediates, ignore, byte
        );
    }
}

// ■ ~ ▲
pub fn process(process: Process, arc_m: &Arc<Mutex<String>>) {
    let reader = BufReader::new(process);

    let mut statemachine = Parser::new();
    let mut performer = Log::new(arc_m);

    for byte in reader.bytes() {
        statemachine.advance(&mut performer, *byte.as_ref().unwrap());
        // let bs = crate::shared::utils::convert_to_utf8_string(byte.unwrap());
        // let mut a = arc_m.lock().unwrap();
        // *a = format!("{}{}", *a, bs);
    }
}
