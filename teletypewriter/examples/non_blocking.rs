#[cfg(unix)]
fn main() {
    use std::io;
    use std::sync::mpsc::Receiver;
    use std::sync::mpsc::TryRecvError;

    let stdin_channel = spawn_stdin_channel();
    loop {
        match stdin_channel.try_recv() {
            Ok(key) => println!("Received: {key}"),
            Err(TryRecvError::Empty) => println!("Channel empty"),
            Err(TryRecvError::Disconnected) => panic!("Channel disconnected"),
        }
        sleep(1000);
    }
}

#[cfg(not(unix))]
fn main() {}

#[cfg(unix)]
fn spawn_stdin_channel() -> Receiver<String> {
    let (tx, rx) = std::mpsc::channel::<String>();
    thread::spawn(move || loop {
        let mut buffer = String::new();
        io::stdin().read_line(&mut buffer).unwrap();
        tx.send(buffer).unwrap();
    });
    rx
}

#[cfg(unix)]
fn sleep(millis: u64) {
    use std::{thread, time};
    let duration = time::Duration::from_millis(millis);
    thread::sleep(duration);
}
