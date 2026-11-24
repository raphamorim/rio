//! Rio's config file, kept live: reloaded when the file changes, and its
//! colors following the desktop's light or dark style when it has
//! adaptive colors.

use crate::shared::Shared;
use gtk::gio;
use gtk::prelude::*;
use rio_backend::config::theme::AppearanceTheme;
use rio_backend::config::Config;
use std::rc::Rc;

pub fn load() -> Config {
    match Config::try_load() {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!("using the default config: {err:?}");
            Config::default()
        }
    }
}

/// `base` with the colors for the light or dark style applied.
pub fn effective(base: &Config, dark: bool) -> Config {
    let mut config = base.clone();
    let dark = match base.force_theme {
        Some(AppearanceTheme::Dark) => true,
        Some(AppearanceTheme::Light) => false,
        None => dark,
    };
    if let Some(adaptive) = &base.adaptive_colors {
        let colors = if dark { adaptive.dark } else { adaptive.light };
        if let Some(colors) = colors {
            config.colors = colors;
        }
    }
    config
}

/// The window chrome's style: the desktop's when the terminal follows
/// it, otherwise the one that matches the terminal's background.
pub fn color_scheme(base: &Config) -> adw::ColorScheme {
    match base.force_theme {
        Some(AppearanceTheme::Dark) => return adw::ColorScheme::ForceDark,
        Some(AppearanceTheme::Light) => return adw::ColorScheme::ForceLight,
        None => {}
    }
    if base.adaptive_colors.is_some() {
        return adw::ColorScheme::Default;
    }
    let [r, g, b, _] = base.colors.background.0;
    let luminance = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if luminance < 0.5 {
        adw::ColorScheme::ForceDark
    } else {
        adw::ColorScheme::ForceLight
    }
}

/// Follow the config file and the desktop style for as long as the app
/// runs.
pub fn watch(shared: &Rc<Shared>) -> Option<gio::FileMonitor> {
    let style = adw::StyleManager::default();
    style.set_color_scheme(color_scheme(&shared.base.borrow()));
    let weak = Rc::downgrade(shared);
    style.connect_dark_notify(move |style| {
        if let Some(shared) = weak.upgrade() {
            shared.restyle(style.is_dark());
        }
    });

    let file = gio::File::for_path(rio_backend::config::config_file_path());
    let monitor = file
        .monitor_file(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        .ok()?;
    monitor.set_rate_limit(200);
    let weak = Rc::downgrade(shared);
    monitor.connect_changed(move |_, _, _, event| {
        use gio::FileMonitorEvent as E;
        if !matches!(
            event,
            E::Changed | E::ChangesDoneHint | E::Created | E::MovedIn | E::Renamed
        ) {
            return;
        }
        let Some(shared) = weak.upgrade() else {
            return;
        };
        match Config::try_load() {
            Ok(config) => shared.reload(config),
            Err(err) => tracing::warn!("keeping the current config: {err:?}"),
        }
    });
    Some(monitor)
}
