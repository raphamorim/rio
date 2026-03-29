/// Send a desktop notification using the platform's native API.
///
/// - **macOS**: `UNUserNotificationCenter` (requires app bundle with identifier).
/// - **Linux**: D-Bus `org.freedesktop.Notifications`.
/// - **Windows**: Toast notifications via `windows` crate.
///
/// Spawns a background thread so the caller is never blocked.
pub fn send_notification(title: &str, body: &str) {
    let title = if title.is_empty() {
        "Rio".to_string()
    } else {
        title.to_string()
    };
    let body = body.to_string();

    std::thread::spawn(move || {
        platform::notify(&title, &body);
    });
}

#[cfg(target_os = "macos")]
mod platform {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use objc2_foundation::NSString;
    use objc2_user_notifications::{
        UNMutableNotificationContent, UNNotificationRequest, UNUserNotificationCenter,
    };

    pub fn notify(title: &str, body: &str) {
        unsafe {
            // UNUserNotificationCenter crashes if the app has no bundle
            // identifier (e.g. cargo run). Guard like Kitty does.
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            if bundle.is_null() {
                return;
            }
            let bundle_id: *mut Object = msg_send![bundle, bundleIdentifier];
            if bundle_id.is_null() {
                return;
            }

            let center = UNUserNotificationCenter::currentNotificationCenter();

            let content = UNMutableNotificationContent::new();
            content.setTitle(&NSString::from_str(title));
            content.setBody(&NSString::from_str(body));

            let identifier = NSString::from_str("rio-notification");
            let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
                &identifier,
                &content,
                None,
            );

            center.addNotificationRequest_withCompletionHandler(&request, None);
        }
    }
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
mod platform {
    use std::collections::HashMap;

    pub fn notify(title: &str, body: &str) {
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
                "Rio",          // app_name
                0u32,           // replaces_id
                "rio",          // app_icon
                title,          // summary
                body,           // body
                &[] as &[&str], // actions
                &hints,         // hints
                -1i32,          // expire_timeout
            ),
        );
    }
}

#[cfg(target_os = "windows")]
mod platform {
    pub fn notify(title: &str, body: &str) {
        use windows::core::HSTRING;
        use windows::Data::Xml::Dom::XmlDocument;
        use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

        let Ok(xml) = XmlDocument::new() else {
            return;
        };
        let toast_xml = format!(
            r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
            title, body,
        );
        if xml.LoadXml(&HSTRING::from(&toast_xml)).is_err() {
            return;
        }
        let Ok(toast) = ToastNotification::CreateToastNotification(&xml) else {
            return;
        };
        let Ok(notifier) =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from("Rio"))
        else {
            return;
        };
        let _ = notifier.Show(&toast);
    }
}
