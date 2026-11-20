//! Notifications through the `org.freedesktop.Notifications` D-Bus service.

use crate::notification::Notification;
use std::borrow::Cow;
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
    // A server with the `body-markup` capability reads the body as markup
    // (`<b>`, `<a href>`, `<img>`), and the body can come from terminal
    // output, so it is escaped there. A server without it shows the body as
    // it is, where an escaped `&amp;` would be printed literally. The summary
    // is plain text for every server.
    let capabilities: Result<Vec<String>, _> = proxy.call("GetCapabilities", &());
    let body_markup = capabilities
        .map(|capabilities| capabilities.iter().any(|name| name == "body-markup"))
        .unwrap_or(false);
    let body = if body_markup {
        escape_markup(&notification.body)
    } else {
        Cow::Borrowed(notification.body.as_str())
    };
    let hints: HashMap<&str, zbus::zvariant::Value<'_>> = HashMap::new();
    let _: Result<u32, _> = proxy.call(
        "Notify",
        &(
            notification.app_name.as_deref().unwrap_or(""), // app_name
            0u32,                                           // replaces_id
            notification.icon.as_deref().unwrap_or(""),     // app_icon
            notification.title.as_str(),                    // summary
            body.as_ref(),                                  // body
            &[] as &[&str],                                 // actions
            &hints,                                         // hints
            -1i32,                                          // expire_timeout
        ),
    );
}

/// Escape `text` so a server that reads the body as markup shows it as
/// written and cannot be handed a tag.
fn escape_markup(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '<', '>']) {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    for c in text.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            c => escaped.push(c),
        }
    }
    Cow::Owned(escaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_borrowed() {
        assert!(matches!(escape_markup("build finished"), Cow::Borrowed(_)));
    }

    #[test]
    fn escapes_markup() {
        assert_eq!(
            escape_markup(r#"<a href="http://x">a && b</a>"#),
            r#"&lt;a href="http://x"&gt;a &amp;&amp; b&lt;/a&gt;"#
        );
    }

    #[test]
    fn an_entity_is_not_read_as_one() {
        assert_eq!(escape_markup("&lt;b&gt;"), "&amp;lt;b&amp;gt;");
    }
}
