// todo: https://web.mit.edu/rust-lang_v1.25/arch/amd64_ubuntu1404/share/doc/rust/html/std/io/struct.Chars.html

#[cfg(unix)]
fn main() -> std::io::Result<()> {
    use std::borrow::Cow;
    use std::io::Read;
    use std::io::Write;
    // use std::io::BufRead;
    use std::io::BufReader;
    use teletypewriter::{create_pty_with_fork, ProcessReadWrite, Pty};

    let shell = Cow::Borrowed("bash");
    let mut process: Pty = create_pty_with_fork(&shell, 80, 25)?;

    process.writer().write_all(b"1").unwrap();
    process.writer().write_all(b"2").unwrap();
    process.writer().write_all(b"ls\n").unwrap();
    process.writer().write_all(b"echo 1\n").unwrap();

    // let mut reader = BufReader::new(process);
    // let mut stream = BufWriter::new(process_w);
    // let mut line = String::new();
    // let reader = BufReader::new(process);
    // for output in reader.chars() {
    //     println!("{:?}", output);
    // }

    // println!("{:?}", stream);
    // let mut reader = BufReader::new(process);
    // loop {
    //     let _len = reader.read_line(&mut line);
    //     println!("> {:?}", line);
    // }

    let reader = BufReader::new(process);
    for bs in reader.bytes() {
        let u = [bs.unwrap()];
        println!("{:?}", String::from_utf8_lossy(&u));
    }

    Ok(())
}

#[cfg(not(unix))]
fn main() {}
