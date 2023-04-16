use log::{Metadata, Record};

pub struct Logger;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        // If declarative wants to ignore trace
        // metadata.level() <= log::Level::Debug

        true
    }

    fn log(&self, record: &Record) {
        // For cases where wants to validate if log is enabled
        // if self.enabled(record.metadata()) {
        //     println!("{}", record.level());
        // }

        let line = format!(
            "\x1b[35m[{}]\x1b[0m \x1b[34m{}\x1b[0m {}\0",
            record.level(),
            record.target(),
            record.args()
        );
        println!("{line}");
    }

    fn flush(&self) {}
}
