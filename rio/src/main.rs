// With the default subsystem, 'console', windows creates an additional console
// window for the program.
// This is silently ignored on non-windows systems.
// See https://msdn.microsoft.com/en-us/library/4cc7ya5b.aspx for more details.
#![windows_subsystem = "windows"]

mod ansi;
mod cli;
mod clipboard;
mod crosswords;
mod event;
mod ime;
mod logger;
mod ui;
#[cfg(windows)]
mod panic;
mod performer;
mod platform;
mod router;
mod scheduler;
mod screen;
mod selection;
mod sequencer;
mod watch;
use crate::event::EventP;
use crate::sequencer::Sequencer;
use log::{info, LevelFilter, SetLoggerError};
use logger::Logger;
use std::str::FromStr;

#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS,
};

pub fn setup_environment_variables(config: &config::Config) {
    #[cfg(unix)]
    let terminfo = if teletypewriter::terminfo_exists("rio") {
        "rio"
    } else {
        "xterm-256color"
    };

    #[cfg(unix)]
    {
        info!("[setup_environment_variables] terminfo: {terminfo}");
        std::env::set_var("TERM", terminfo);
    }

    std::env::set_var("RIO_CONFIG", config::config_file_path());

    // https://github.com/raphamorim/rio/issues/200
    std::env::set_var("TERM_PROGRAM", "rio");
    std::env::set_var("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

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
    #[cfg(windows)]
    panic::attach_handler();

    // When linked with the windows subsystem windows won't automatically attach
    // to the console of the parent process, so we do it explicitly. This fails
    // silently if the parent has no console.
    #[cfg(windows)]
    unsafe {
        AttachConsole(ATTACH_PARENT_PROCESS);
    }

    // Load command line options.
    let options = cli::Options::new();

    let mut config_error: Option<config::ConfigError> = None;
    let mut config = match config::Config::try_load() {
        Ok(config) => config,
        Err(error) => {
            config_error = Some(error);
            config::Config::default()
        }
    };

    let setup_logs = setup_logs_by_filter_level(&config.developer.log_level);
    if setup_logs.is_err() {
        println!("unable to configure log level");
    }

    if let Some(command) = options.window_options.terminal_options.command() {
        config.shell = command;
        config.use_fork = false;
    }

    setup_environment_variables(&config);

    let window_event_loop =
        winit::event_loop::EventLoopBuilder::<EventP>::with_user_event()
            .build()
            .unwrap();

    let mut sequencer = Sequencer::new(config, config_error);
    let _ = sequencer.run(window_event_loop).await;

    #[cfg(windows)]
    unsafe {
        FreeConsole();
    }

    Ok(())
}
