//! Notifications through the `org.freedesktop.Notifications` D-Bus service.

use crate::notification::Notification;
use std::collections::HashMap;

pub(crate) fn show(notification: &Notification) {
    let Ok(connection) = zbus::blocking::Connection::session() else {
        return;
    };
    let Ok(proxy) = zbus::blocking::Proxy::new(
        &connection,
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
    ) else {
        return;
    };
    let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    let _: Result<u32, _> = proxy.call(
        "Notify",
        &(
            notification.app_name.as_deref().unwrap_or(""), // app_name
            0u32,                                           // replaces_id
            notification.icon.as_deref().unwrap_or(""),     // app_icon
            notification.title.as_str(),                    // summary
            notification.body.as_str(),                     // body
            &[] as &[&str],                                 // actions
            &hints,                                         // hints
            -1i32,                                          // expire_timeout
        ),
    );
}
