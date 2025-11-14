//! Background monitor for system theme changes via XDG Desktop Portal.
//!
//! This module sets up a listener for the SettingsChanged signal from the
//! XDG Desktop Portal, specifically monitoring for color-scheme preference changes.

use crate::window::Theme;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

// Cache for the current theme (0 = None, 1 = Dark, 2 = Light)
static CACHED_THEME: AtomicU8 = AtomicU8::new(0);

/// Get the cached theme without blocking
pub fn get_cached_theme() -> Option<Theme> {
    match CACHED_THEME.load(Ordering::Relaxed) {
        1 => Some(Theme::Dark),
        2 => Some(Theme::Light),
        _ => None,
    }
}

pub(crate) fn set_cached_theme(theme: Option<Theme>) {
    let value = match theme {
        Some(Theme::Dark) => 1,
        Some(Theme::Light) => 2,
        None => 0,
    };
    CACHED_THEME.store(value, Ordering::Relaxed);
}

/// Starts monitoring for system theme changes in a background thread.
///
/// When the system color scheme preference changes (dark/light), the provided
/// callback will be invoked. This allows applications to respond to theme
/// changes in real-time without needing to restart.
///
/// # Arguments
///
/// * `on_change` - Callback function to invoke when theme changes are detected
///
/// # Returns
///
/// Returns `Ok(())` if the monitor was successfully started, or `Err` if
/// the XDG Desktop Portal is not available or there was an error setting up
/// the signal listener.
pub fn start_theme_monitor<F>(on_change: F) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn() + Send + Sync + 'static,
{
    let callback = Arc::new(on_change);

    std::thread::spawn(move || {
        // Create a new tokio runtime for this thread
        let rt = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(_) => return,
        };

        rt.block_on(async {
            let _ = monitor_theme_changes(callback).await;
        });
    });

    Ok(())
}

async fn monitor_theme_changes<F>(
    on_change: Arc<F>,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn() + Send + Sync + 'static,
{
    use ashpd::zbus::fdo::DBusProxy;
    use ashpd::zbus::{Connection, MatchRule, MessageStream};
    use futures_util::stream::StreamExt;

    // Connect to session bus
    let connection = Connection::session().await?;

    // Create match rule for Settings.SettingChanged signal
    let match_rule = MatchRule::builder()
        .msg_type(ashpd::zbus::message::Type::Signal)
        .interface("org.freedesktop.portal.Settings")?
        .member("SettingChanged")?
        .build();

    let dbus_proxy = DBusProxy::new(&connection).await?;
    dbus_proxy.add_match_rule(match_rule.clone()).await?;

    // Create message stream
    let mut stream =
        MessageStream::for_match_rule(match_rule, &connection, Some(100)).await?;

    // Process signals as they arrive
    while let Some(msg) = stream.next().await {
        let msg = msg?;

        // Try to parse the signal arguments
        if let Ok((namespace, key, value)) =
            msg.body()
                .deserialize::<(String, String, ashpd::zbus::zvariant::Value)>()
        {
            if namespace == "org.freedesktop.appearance" && key == "color-scheme" {
                // Extract the theme value (uint32: 0=no pref, 1=dark, 2=light)
                if let Ok(variant) = value.downcast::<ashpd::zbus::zvariant::Value>() {
                    if let Ok(scheme_value) = variant.downcast::<u32>() {
                        let theme = match scheme_value {
                            1 => Some(Theme::Dark),
                            2 => Some(Theme::Light),
                            _ => None,
                        };
                        set_cached_theme(theme);
                        // Invoke the callback to notify the application
                        on_change();
                    }
                }
            }
        }
    }

    Ok(())
}
