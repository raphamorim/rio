//! XDG Desktop Portal integration for reading system preferences.
//!
//! This module provides access to system settings via the XDG Desktop Portal,
//! which is the standard cross-desktop API on Linux systems.

use crate::window::Theme;

/// Queries the system color scheme preference via XDG Desktop Portal.
///
/// This function uses the org.freedesktop.portal.Settings interface to read
/// the color-scheme preference from org.freedesktop.appearance namespace.
///
/// The color-scheme value is a uint32 where:
/// - 0: No preference
/// - 1: Prefer dark appearance
/// - 2: Prefer light appearance
///
/// Returns `None` if the portal is not available, the setting doesn't exist,
/// or if there's an error querying the portal.
pub fn get_color_scheme() -> Option<Theme> {
    // Use blocking API since this is called during event loop initialization
    // and we need the result immediately
    let result = std::panic::catch_unwind(|| {
        tokio::runtime::Runtime::new()
            .ok()?
            .block_on(async { query_color_scheme_async().await })
    });

    let theme = match result {
        Ok(Some(theme)) => Some(theme),
        _ => None,
    };

    // Also update the cached theme
    super::theme_monitor::set_cached_theme(theme);
    theme
}

async fn query_color_scheme_async() -> Option<Theme> {
    use ashpd::desktop::settings::{ColorScheme, Settings};

    let settings = Settings::new().await.ok()?;
    let color_scheme = settings.color_scheme().await.ok()?;

    match color_scheme {
        ColorScheme::PreferDark => Some(Theme::Dark),
        ColorScheme::PreferLight => Some(Theme::Light),
        ColorScheme::NoPreference => None,
    }
}
