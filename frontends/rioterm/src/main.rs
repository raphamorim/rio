// With the default subsystem, 'console', windows creates an additional console
// window for the program.
// This is silently ignored on non-windows systems.
// See https://msdn.microsoft.com/en-us/library/4cc7ya5b.aspx for more details.
#![windows_subsystem = "windows"]

mod application;
mod bindings;
mod cli;
mod constants;
mod context;
mod ime;
mod messenger;
mod mouse;
#[cfg(windows)]
mod panic;
mod platform;
mod renderer;
mod router;
mod routes;
mod scheduler;
mod screen;
mod watcher;

use clap::Parser;
use rio_backend::event::EventPayload;
use rio_backend::{ansi, crosswords, event, performer, selection};
use std::str::FromStr;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    self, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

#[cfg(windows)]
use windows_sys::Win32::System::Console::{
    AttachConsole, FreeConsole, ATTACH_PARENT_PROCESS,
};

const LOG_LEVEL_ENV: &str = "RIO_LOG_LEVEL";

pub fn setup_environment_variables(config: &rio_backend::config::Config) {
    #[cfg(unix)]
    let terminfo = if teletypewriter::terminfo_exists("rio") {
        "rio"
    } else {
        "xterm-256color"
    };

    #[cfg(unix)]
    {
        let span = tracing::span!(tracing::Level::INFO, "setup_environment_variables");
        let _guard = span.enter();
        tracing::info!("terminfo: {terminfo}");
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

fn setup_logs_by_filter_level(
    log_level: &str,
    log_file: &Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut filter_level = LevelFilter::from_str(log_level).unwrap_or(LevelFilter::OFF);

    if let Ok(data) = std::env::var(LOG_LEVEL_ENV) {
        if !data.is_empty() {
            filter_level = LevelFilter::from_str(&data).unwrap_or(filter_level);
        }
    }

    let filter = EnvFilter::builder()
        .with_default_directive(filter_level.into())
        .parse("")?;

    let stdout_subscriber = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .with_ansi(true)
        .with_filter(filter);
    let subscriber = tracing_subscriber::registry().with(stdout_subscriber);

    if let Some(log_file) = &log_file {
        let log_file = std::fs::File::create(log_file)?;
        let file_subscriber = tracing_subscriber::fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_writer(log_file)
            .with_target(false)
            .with_ansi(false);
        subscriber.with(file_subscriber).init();
    } else {
        subscriber.init();
    }

    let span = tracing::span!(tracing::Level::INFO, "logger");
    let _guard = span.enter();
    tracing::info!("log_level: {log_level}");
    if let Some(log_file) = log_file {
        tracing::info!("log_file: {log_file}");
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
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

    let write_config_path = args.window_options.terminal_options.write_config.clone();
    if let Some(config_path) = write_config_path {
        let _ = setup_logs_by_filter_level("TRACE", &None);
        rio_backend::config::create_config_file(config_path);
        return Ok(());
    }

    let (mut config, config_error) = match rio_backend::config::Config::try_load() {
        Ok(config) => (config, None),
        Err(err) => (rio_backend::config::Config::default(), Some(err)),
    };

    {
        if let Err(e) = setup_logs_by_filter_level(
            &config.developer.log_level,
            &config.developer.log_file,
        ) {
            eprintln!("unable to configure the logger: {e:?}");
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

    let window_event_loop =
        rio_window::event_loop::EventLoop::<EventPayload>::with_user_event().build()?;

    let mut application =
        crate::application::Application::new(config, config_error, &window_event_loop);
    let _ = application.run(window_event_loop);

    #[cfg(windows)]
    unsafe {
        FreeConsole();
    }

    Ok(())
}
