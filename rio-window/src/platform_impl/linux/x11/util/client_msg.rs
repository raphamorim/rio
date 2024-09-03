use super::*;
use x11rb::x11_utils::Serialize;

impl XConnection {
    pub fn send_client_msg(
        &self,
        window: xproto::Window, // The window this is "about"; not necessarily this window
        target_window: xproto::Window, // The window we're sending to
        message_type: xproto::Atom,
        event_mask: Option<xproto::EventMask>,
        data: impl Into<xproto::ClientMessageData>,
    ) -> Result<VoidCookie<'_>, X11Error> {
        let event = xproto::ClientMessageEvent {
            response_type: xproto::CLIENT_MESSAGE_EVENT,
            window,
            format: 32,
            data: data.into(),
            sequence: 0,
            type_: message_type,
        };

        self.xcb_connection()
            .send_event(
                false,
                target_window,
                event_mask.unwrap_or(xproto::EventMask::NO_EVENT),
                event.serialize(),
            )
            .map_err(Into::into)
    }
}
