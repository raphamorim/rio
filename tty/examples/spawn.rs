use std::env;
use std::io::{BufReader, BufRead};
use std::borrow::Cow;

use tty::{COLS, ROWS, pty};

fn main() -> std::io::Result<()> {
    // Set the TERM variable
    env::set_var("TERM", "rio");

    // let shell = Cow::Borrowed("bash");
    let shell = Cow::Borrowed("bash");
    let (process, pid) = pty(&shell, COLS as u16, ROWS as u16);

    println!("{:?}", pid);

    let mut reader = BufReader::new(process);
    let mut line = String::new();
    loop {
        let _len = reader.read_line(&mut line);
        println!("> {:?}", line);
    }

    Ok(())
}
