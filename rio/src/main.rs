extern crate clap;

use clap::{Arg, App};

fn main() {
    let matches = App::new("Rio")
        .version("0.1.0")
        .author("Raphael Amorim <rapha850@gmail.com>")
        .about("JavaScript Package Manager")
        .arg(Arg::with_name("install")
                 .required(true)
                 .takes_value(true)
                 .index(1)
                 .help("install packages"))
        .get_matches();

    // println!("Value for --output: {}", matches.value_of("install").unwrap());

    if matches.is_present("install") {
        println!("nice");
    }
}
