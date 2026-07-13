//! Background monitor for system color-scheme changes via the XDG Desktop Portal.
//!
//! A dedicated thread reads `org.freedesktop.appearance` / `color-scheme` once to
//! seed a cache, then blocks on the portal's `SettingChanged` signal. On every
//! update it stores the new value and wakes the event loop through a calloop
//! `Ping`; the loop then emits `WindowEvent::ThemeChanged` for each window (see
//! the wayland/x11 backends).

use crate::window::Theme;
use calloop::ping::Ping;
use std::sync::atomic::{AtomicU8, Ordering};

// Last known system color-scheme: 0 = no preference / unknown, 1 = dark, 2 = light.
static CACHED_THEME: AtomicU8 = AtomicU8::new(0);

const PORTAL_DESTINATION: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const SETTINGS_INTERFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE_NAMESPACE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME_KEY: &str = "color-scheme";

#[inline]
fn store(theme: Option<Theme>) {
    let value = match theme {
        Some(Theme::Dark) => 1,
        Some(Theme::Light) => 2,
        None => 0,
    };
    CACHED_THEME.store(value, Ordering::Relaxed);
}

#[inline]
pub fn get_cached_theme() -> Option<Theme> {
    match CACHED_THEME.load(Ordering::Relaxed) {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

#[inline]
pub fn start_theme_monitor(waker: Ping) {
    let spawned = std::thread::Builder::new()
        .name("rio-theme-monitor".into())
        .spawn(move || {
            if let Err(err) = run(&waker) {
                tracing::warn!("theme monitor exited: {err}");
            }
        });

    if let Err(err) = spawned {
        tracing::warn!("failed to spawn theme monitor: {err}");
    }
}

fn run(waker: &Ping) -> Result<(), zbus::Error> {
    let connection = zbus::blocking::Connection::session()?;
    let proxy = zbus::blocking::Proxy::new(
        &connection,
        PORTAL_DESTINATION,
        PORTAL_PATH,
        SETTINGS_INTERFACE,
    )?;

    store(read_color_scheme(&proxy));
    waker.ping();

    // Block on the SettingChanged signal stream for the rest of the process.
    for message in proxy.receive_signal("SettingChanged")? {
        let Ok((namespace, key, value)) =
            message
                .body()
                .deserialize::<(String, String, zbus::zvariant::OwnedValue)>()
        else {
            continue;
        };

        if namespace != APPEARANCE_NAMESPACE || key != COLOR_SCHEME_KEY {
            continue;
        }

        store(theme_from_value(&value));
        waker.ping();
    }

    Ok(())
}

#[inline]
fn read_color_scheme(proxy: &zbus::blocking::Proxy) -> Option<Theme> {
    let value: zbus::zvariant::OwnedValue =
        match proxy.call("ReadOne", &(APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY)) {
            Ok(value) => value,
            Err(_) => proxy
                .call("Read", &(APPEARANCE_NAMESPACE, COLOR_SCHEME_KEY))
                .ok()?,
        };
    theme_from_value(&value)
}

#[inline]
fn theme_from_value(value: &zbus::zvariant::OwnedValue) -> Option<Theme> {
    match value.downcast_ref::<u32>() {
        Ok(1) => Some(Theme::Dark),
        Ok(2) => Some(Theme::Light),
        _ => None,
    }
}
