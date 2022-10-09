use std::borrow::Cow;
use std::env;
use std::io::Write;
use std::io::{BufRead, BufReader};
use tty::{pty, COLS, ROWS};

fn main() -> std::io::Result<()> {
    env::set_var("TERM", "rio");

    let shell = Cow::Borrowed("bash");
    let (process, mut w, pid) = pty(&shell, COLS as u16, ROWS as u16);
    println!("{:?}", pid);

    // let mut reader = BufReader::new(process);
    // let mut stream = BufWriter::new(process_w);
    // w.write_all(b"");
    // w.write_all(b"ls\n");

    // w.write_all(b"echo 1\n");
    let mut line = String::new();

    let reader = BufReader::new(process);
    for output in reader.lines() {
        println!("{:?}", output);
    }

    // println!("{:?}", stream);
    // let mut reader = BufReader::new(process);
    // loop {
    //     let _len = reader.read_line(&mut line);
    //     println!("> {:?}", line);
    // }

    Ok(())
}
