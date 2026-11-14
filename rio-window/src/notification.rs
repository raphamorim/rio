//! Desktop notifications.
//!
//! A [`Notification`] is posted through the platform's native service:
//!
//! - **macOS**: `UNUserNotificationCenter`. The process must run from an app
//!   bundle with a bundle identifier; otherwise nothing is shown. Call
//!   [`request_permission`] once at startup so the user is prompted.
//! - **Linux / BSD**: the `org.freedesktop.Notifications` D-Bus service.
//! - **Windows**: a toast notification.
//! - **Web / Redox**: unsupported, [`Notification::show`] does nothing.
//!
//! ```no_run
//! use rio_window::notification::{self, Notification};
//!
//! notification::request_permission();
//!
//! Notification::new("Build finished")
//!     .with_body("cargo build took 42s.")
//!     .with_app_name("Rio")
//!     .with_icon("rio")
//!     .show();
//! ```
//!
//! Posting is fire-and-forget: [`Notification::show`] returns immediately and
//! can be called from any thread, with or without a running event loop. A
//! notification the platform refuses (no permission, no notification daemon,
//! no bundle identifier) is dropped silently.

/// Ask the user for permission to post notifications.
///
/// On macOS this shows the system permission prompt the first time it is
/// called for the app; later calls do nothing. Other platforms need no
/// permission, so there it is a no-op.
pub fn request_permission() {
    #[cfg(macos_platform)]
    crate::platform_impl::notification::request_permission();
}

/// A desktop notification, built with the `with_*` methods and posted with
/// [`show`](Self::show).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) app_name: Option<String>,
    pub(crate) icon: Option<String>,
}

impl Notification {
    /// Create a notification with the given title and an empty body.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: String::new(),
            app_name: None,
            icon: None,
        }
    }

    /// Set the body text shown under the title.
    #[inline]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    /// Set the name of the application posting the notification.
    ///
    /// ## Platform-specific
    ///
    /// - **Linux / BSD:** sent as the `app_name` of the notification.
    /// - **Windows:** used as the Application User Model ID. An app without
    ///   package identity must set it, or no toast is shown.
    /// - **macOS:** ignored, the system uses the app bundle.
    #[inline]
    pub fn with_app_name(mut self, app_name: impl Into<String>) -> Self {
        self.app_name = Some(app_name.into());
        self
    }

    /// Set the icon, as a name from the icon theme or a `file://` URI.
    ///
    /// ## Platform-specific
    ///
    /// - **macOS / Windows:** ignored, the system uses the app's own icon.
    #[inline]
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// The title.
    #[inline]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The body text.
    #[inline]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// The application name, if one was set.
    #[inline]
    pub fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    /// The icon, if one was set.
    #[inline]
    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    /// Post the notification.
    ///
    /// Returns immediately; the platform call runs on a background thread.
    pub fn show(self) {
        #[cfg(any(macos_platform, windows_platform, x11_platform, wayland_platform))]
        {
            let spawned = std::thread::Builder::new()
                .name("rio-notification".into())
                .spawn(move || crate::platform_impl::notification::show(&self));

            if let Err(err) = spawned {
                tracing::warn!("failed to spawn notification thread: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_only_a_title() {
        let notification = Notification::new("title");
        assert_eq!(notification.title(), "title");
        assert_eq!(notification.body(), "");
        assert_eq!(notification.app_name(), None);
        assert_eq!(notification.icon(), None);
    }

    #[test]
    fn builders_set_each_field() {
        let notification = Notification::new("title")
            .with_body("body")
            .with_app_name("Rio")
            .with_icon("rio");
        assert_eq!(notification.title(), "title");
        assert_eq!(notification.body(), "body");
        assert_eq!(notification.app_name(), Some("Rio"));
        assert_eq!(notification.icon(), Some("rio"));
    }
}
