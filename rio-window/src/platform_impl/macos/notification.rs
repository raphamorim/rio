//! Notifications through `UNUserNotificationCenter`.

use crate::notification::Notification;
use block2::RcBlock;
use objc::runtime::Object;
use objc::{class, msg_send, sel, sel_impl};
use objc2::runtime::Bool;
use objc2_foundation::{NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNUserNotificationCenter,
};
use std::sync::Once;

/// `UNUserNotificationCenter` crashes if the app has no bundle identifier
/// (e.g. `cargo run`). Guard like Kitty does.
fn has_bundle_identifier() -> bool {
    unsafe {
        let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return false;
        }
        let bundle_id: *mut Object = msg_send![bundle, bundleIdentifier];
        !bundle_id.is_null()
    }
}

pub(crate) fn request_permission() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if !has_bundle_identifier() {
            return;
        }

        unsafe {
            let center = UNUserNotificationCenter::currentNotificationCenter();
            center.requestAuthorizationWithOptions_completionHandler(
                UNAuthorizationOptions::UNAuthorizationOptionAlert
                    | UNAuthorizationOptions::UNAuthorizationOptionSound,
                &RcBlock::new(|_ok: Bool, _err: *mut NSError| {}),
            );
        }
    });
}

pub(crate) fn show(notification: &Notification) {
    if !has_bundle_identifier() {
        return;
    }

    unsafe {
        let center = UNUserNotificationCenter::currentNotificationCenter();

        let content = UNMutableNotificationContent::new();
        content.setTitle(&NSString::from_str(&notification.title));
        content.setBody(&NSString::from_str(&notification.body));

        let identifier = NSString::from_str("rio-notification");
        let request = UNNotificationRequest::requestWithIdentifier_content_trigger(
            &identifier,
            &content,
            None,
        );

        center.addNotificationRequest_withCompletionHandler(&request, None);
    }
}
