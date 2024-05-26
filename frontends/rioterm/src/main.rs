// With the default subsystem, 'console', windows creates an additional console
// window for the program.
// This is silently ignored on non-windows systems.
// See https://msdn.microsoft.com/en-us/library/4cc7ya5b.aspx for more details.
#![windows_subsystem = "windows"]

#[cfg(use_wa)]
mod app;
mod bindings;
mod cli;
mod constants;
mod context;
mod ime;
mod logger;
mod messenger;
mod mouse;
#[cfg(windows)]
mod panic;
mod platform;
mod renderer;
#[cfg(not(use_wa))]
mod router;
mod routes;
mod scheduler;
#[cfg(not(use_wa))]
mod screen;
#[cfg(not(use_wa))]
mod sequencer;
mod state;
mod watcher;

use clap::Parser;
use log::{info, LevelFilter, SetLoggerError};
use logger::Logger;
use rio_backend::{ansi, crosswords, event, performer, selection};
use std::str::FromStr;

#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS,
};

pub fn setup_environment_variables(config: &rio_backend::config::Config) {
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

    // https://github.com/raphamorim/rio/issues/200
    std::env::set_var("TERM_PROGRAM", "rio");
    std::env::set_var("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));

    std::env::set_var("COLORTERM", "truecolor");
    std::env::remove_var("DESKTOP_STARTUP_ID");
    std::env::remove_var("XDG_ACTIVATION_TOKEN");
    #[cfg(target_os = "macos")]
    {
        platform::macos::set_locale_environment();
        std::env::set_current_dir(dirs::home_dir().unwrap()).unwrap();
    }

    // Set env vars from config.
    for env_config in config.env_vars.iter() {
        let env_vec: Vec<&str> = env_config.split('=').collect();

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
    let args = cli::Cli::parse();

    let (mut config, config_error) = match rio_backend::config::Config::try_load() {
        Ok(config) => (config, None),
        Err(err) => (rio_backend::config::Config::default(), Some(err)),
    };

    {
        if setup_logs_by_filter_level(&config.developer.log_level).is_err() {
            eprintln!("unable to configure log level");
        }

        if let Some(command) = args.window_options.terminal_options.command() {
            config.shell = command;
            config.use_fork = false;
        }

        if let Some(working_dir_cli) = args.window_options.terminal_options.working_dir {
            config.working_dir = Some(working_dir_cli);
        }
    }

    #[cfg(target_os = "linux")]
    {
        // If running inside a flatpak sandbox.
        // Rio will never use use_fork configuration as true
        if std::path::PathBuf::from("/.flatpak-info").exists() {
            config.use_fork = false;
        }
    }

    setup_environment_variables(&config);

    #[cfg(not(use_wa))]
    {
        let window_event_loop = winit::event_loop::EventLoop::with_user_event()
            .build()
            .unwrap();

        let mut sequencer = crate::sequencer::Sequencer::new(config, config_error);
        let _ = sequencer.run(window_event_loop).await;
    }

    #[cfg(use_wa)]
    let _ = app::run(config, config_error).await;

    #[cfg(windows)]
    unsafe {
        FreeConsole();
    }

    Ok(())
}
