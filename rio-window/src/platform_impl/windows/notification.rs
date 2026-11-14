//! Toast notifications through WinRT.

use crate::notification::Notification;
use windows::core::HSTRING;
use windows::Data::Xml::Dom::XmlDocument;
use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};

pub(crate) fn show(notification: &Notification) {
    let Ok(xml) = XmlDocument::new() else {
        return;
    };
    if xml
        .LoadXml(&HSTRING::from(toast_xml(
            &notification.title,
            &notification.body,
        )))
        .is_err()
    {
        return;
    }
    let Ok(toast) = ToastNotification::CreateToastNotification(&xml) else {
        return;
    };
    let notifier = match notification.app_name.as_deref() {
        Some(app_name) => {
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_name))
        }
        None => ToastNotificationManager::CreateToastNotifier(),
    };
    let Ok(notifier) = notifier else {
        return;
    };
    let _ = notifier.Show(&toast);
}

fn toast_xml(title: &str, body: &str) -> String {
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text><text>{}</text></binding></visual></toast>"#,
        escape_xml(title),
        escape_xml(body),
    )
}

/// Escape `text` for use as XML character data.
///
/// The title and body can come from terminal output, so they must not be able
/// to add elements to the toast. Control characters XML 1.0 forbids are
/// dropped, since one of them would make the whole document fail to load.
fn escape_xml(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            '\t' | '\n' | '\r' => escaped.push(c),
            c if c < ' ' || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => escaped.push(c),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_markup() {
        assert_eq!(
            escape_xml(r#"<a href="x">&'</a>"#),
            "&lt;a href=&quot;x&quot;&gt;&amp;&apos;&lt;/a&gt;"
        );
    }

    #[test]
    fn drops_forbidden_control_characters() {
        assert_eq!(escape_xml("a\x1b[0m\0b\tc\n"), "a[0mb\tc\n");
    }

    #[test]
    fn text_cannot_add_toast_elements() {
        let xml = toast_xml("</text><actions/>", "body");
        assert!(!xml.contains("<actions/>"));
        assert_eq!(xml.matches("<text>").count(), 2);
    }
}
