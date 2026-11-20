//! Rio as a GTK 4 / libadwaita app.
//!
//! The terminal is librio (PTY, parser, state, input encoding), drawn by
//! Sugarloaf's CPU renderer into a frame the terminal widget hands GTK as a
//! texture, so it renders exactly as Rio does. Around it, GNOME's own
//! pieces: an `AdwApplicationWindow` with `AdwTabBar` tabs. Settings come
//! from Rio's config file.

mod input;
mod shared;
mod terminal;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};
use std::rc::Rc;

const APP_ID: &str = "com.rioterm.RioGtk";

fn main() -> glib::ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let app = adw::Application::builder()
        .application_id(APP_ID)
        // Each launch opens a window in the running instance.
        .flags(gio::ApplicationFlags::empty())
        .build();

    let shared: Rc<std::cell::OnceCell<Rc<shared::Shared>>> = Rc::default();
    app.connect_startup({
        let shared = shared.clone();
        move |app| {
            let config = match rio_backend::config::Config::try_load() {
                Ok(config) => config,
                Err(err) => {
                    tracing::warn!("using the default config: {err:?}");
                    rio_backend::config::Config::default()
                }
            };
            let _ = shared.set(shared::Shared::start(config));
            window::install(app);
        }
    });
    app.connect_activate(move |app| {
        if let Some(shared) = shared.get() {
            window::Window::open(app, shared);
        }
    });
    app.run()
}
