mod ansi;
mod clipboard;
mod crosswords;
mod event;
mod ime;
mod layout;
mod logger;
mod performer;
mod platform;
mod scheduler;
mod screen;
mod selection;
mod sequencer;
use crate::event::EventP;
use crate::sequencer::Sequencer;
use log::{info, LevelFilter, SetLoggerError};
use logger::Logger;
use std::str::FromStr;

pub fn setup_environment_variables(config: &config::Config) {
    let terminfo = if teletypewriter::terminfo_exists("rio") {
        "rio"
    } else {
        "xterm-256color"
    };

    info!("[setup_environment_variables] terminfo: {terminfo}");

    std::env::set_var("TERM", terminfo);
    std::env::set_var("COLORTERM", "truecolor");
    std::env::remove_var("DESKTOP_STARTUP_ID");
    #[cfg(target_os = "macos")]
    {
        platform::macos::set_locale_environment();
        std::env::set_current_dir(dirs::home_dir().unwrap()).unwrap();
    }

    // Set env vars from config.
    for env_config in config.env_vars.iter() {
        let mut env_vec = vec![];
        for config in env_config.split('=') {
            env_vec.push(config);
        }

        if env_vec.len() == 2 {
            std::env::set_var(env_vec[0], env_vec[1]);
        }
    }
}

static LOGGER: Logger = Logger;

fn setup_logs_by_filter_level(log_level: LevelFilter) -> Result<(), SetLoggerError> {
    log::set_logger(&LOGGER).map(|()| log::set_max_level(log_level))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = config::Config::load();
    let filter_level =
        LevelFilter::from_str(&config.developer.log_level).unwrap_or(LevelFilter::Off);

    let setup_logs = setup_logs_by_filter_level(filter_level);
    if setup_logs.is_err() {
        println!("unable to configure log level");
    }

    setup_environment_variables(&config);

    let window_event_loop =
        winit::event_loop::EventLoopBuilder::<EventP>::with_user_event().build();
    let mut sequencer = Sequencer::new(config);
    let result = sequencer.run(window_event_loop);

    result.await
}
