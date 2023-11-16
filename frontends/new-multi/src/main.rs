use rio_backend::{ansi, clipboard, crosswords, event, performer, selection};

mod ime;
mod logger;
mod platform;
// mod router;
// mod scheduler;
// mod screen;
mod sequencer;
mod watch;
use crate::event::EventP;
use crate::sequencer::Sequencer;
use log::{info, LevelFilter, SetLoggerError};
use logger::Logger;
use std::str::FromStr;

pub fn setup_environment_variables(config: &rio_backend::config::Config) {
    let terminfo = if teletypewriter::terminfo_exists("rio") {
        "rio"
    } else {
        "xterm-256color"
    };

    info!("[setup_environment_variables] terminfo: {terminfo}");
    std::env::set_var("TERM", terminfo);

    // https://github.com/raphamorim/rio/issues/200
    std::env::set_var("TERM_PROGRAM", "rio");
    std::env::set_var("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

    std::env::set_var("COLORTERM", "truecolor");
    std::env::remove_var("DESKTOP_STARTUP_ID");
    platform::macos::set_locale_environment();
    std::env::set_current_dir(dirs::home_dir().unwrap()).unwrap();

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

fn setup_logs_by_filter_level(log_level: &str) -> Result<(), SetLoggerError> {
    let mut filter_level = LevelFilter::from_str(log_level).unwrap_or(LevelFilter::Off);

    if let Ok(data) = std::env::var("RIO_LOG_LEVEL") {
        if !data.is_empty() {
            filter_level = LevelFilter::from_str(&data).unwrap_or(filter_level);
        }
    }

    info!("[setup_logs_by_filter_level] log_level: {log_level}");
    log::set_logger(&LOGGER).map(|()| log::set_max_level(filter_level))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load command line options.
    // let options = cli::Options::new();

    let mut config_error: Option<rio_backend::config::ConfigError> = None;
    let mut config = match rio_backend::config::Config::try_load() {
        Ok(config) => config,
        Err(error) => {
            config_error = Some(error);
            rio_backend::config::Config::default()
        }
    };

    let setup_logs = setup_logs_by_filter_level(&config.developer.log_level);
    if setup_logs.is_err() {
        println!("unable to configure log level");
    }

    // if let Some(command) = options.window_options.terminal_options.command() {
    //     config.shell = command;
    //     config.use_fork = false;
    // }

    // if let Some(working_dir_cli) = options.window_options.terminal_options.working_dir {
    //     config.working_dir = Some(working_dir_cli);
    // }

    setup_environment_variables(&config);

    // let window_event_loop =
    //     winit::event_loop::EventLoopBuilder::<EventP>::with_user_event()
    //         .build()
    //         .unwrap();

    let mut sequencer = Sequencer::new(config, config_error);
    let _ = sequencer.start();

    Ok(())
}
