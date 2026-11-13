/// Request notification authorization from the OS.
/// On macOS this triggers the permission prompt on first call.
/// No-op on other platforms.
pub fn request_authorization() {
    #[cfg(target_os = "macos")]
    platform::request_authorization();
}

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
    use block2::{Block, RcBlock};
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::{Bool, NSObject, NSObjectProtocol, ProtocolObject};
    use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
    use objc2_foundation::{NSError, NSString};
    use objc2_user_notifications::{
        UNAuthorizationOptions, UNMutableNotificationContent, UNNotification,
        UNNotificationPresentationOptions, UNNotificationRequest,
        UNUserNotificationCenter, UNUserNotificationCenterDelegate,
    };
    use std::sync::Once;

    declare_class!(
        struct NotificationDelegate;

        // SAFETY:
        // - The superclass NSObject does not have any subclassing requirements.
        // - Interior mutability is a safe default.
        // - `NotificationDelegate` does not implement `Drop`.
        unsafe impl ClassType for NotificationDelegate {
            type Super = NSObject;
            type Mutability = mutability::InteriorMutable;
            const NAME: &'static str = "RioNotificationDelegate";
        }

        impl DeclaredClass for NotificationDelegate {
            type Ivars = ();
        }

        unsafe impl NotificationDelegate {
            #[method_id(init)]
            fn init(this: Allocated<Self>) -> Option<Retained<Self>> {
                unsafe { msg_send_id![super(this.set_ivars(())), init] }
            }
        }

        unsafe impl NSObjectProtocol for NotificationDelegate {}

        unsafe impl UNUserNotificationCenterDelegate for NotificationDelegate {
            #[method(userNotificationCenter:willPresentNotification:withCompletionHandler:)]
            fn will_present(
                &self,
                _center: &UNUserNotificationCenter,
                _notification: &UNNotification,
                completion_handler: &Block<dyn Fn(UNNotificationPresentationOptions)>,
            ) {
                completion_handler.call((
                    UNNotificationPresentationOptions::UNNotificationPresentationOptionBanner
                        | UNNotificationPresentationOptions::UNNotificationPresentationOptionList
                        | UNNotificationPresentationOptions::UNNotificationPresentationOptionSound,
                ));
            }
        }
    );

    pub(crate) fn request_authorization() {
        static INIT: Once = Once::new();
        INIT.call_once(|| unsafe {
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            if bundle.is_null() {
                return;
            }
            let bundle_id: *mut Object = msg_send![bundle, bundleIdentifier];
            if bundle_id.is_null() {
                return;
            }

            let center = UNUserNotificationCenter::currentNotificationCenter();

            // macOS does not present a notification posted by the app that is
            // currently frontmost unless the center's delegate asks it to.
            // Terminal notifications are emitted by programs running inside
            // Rio, so Rio is nearly always frontmost when one arrives, and
            // without this every notification is delivered silently to
            // Notification Center with no banner.
            let delegate: Retained<NotificationDelegate> =
                msg_send_id![NotificationDelegate::alloc(), init];
            center.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
            // The center holds its delegate weakly, so keep ours alive for the
            // lifetime of the process.
            std::mem::forget(delegate);

            center.requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::UNAuthorizationOptionAlert
                    | UNAuthorizationOptions::UNAuthorizationOptionSound,
                &RcBlock::new(|_ok: Bool, _err: *mut NSError| {}),
            );
        });
    }

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
