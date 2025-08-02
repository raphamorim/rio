// SPDX-License-Identifier: Apache-2.0

//! X11 activation handling.
//!
//! X11 has a "startup notification" specification similar to Wayland's, see this URL:
//! <https://specifications.freedesktop.org/startup-notification-spec/startup-notification-latest.txt>

use super::atoms::*;
use super::{VoidCookie, X11Error, XConnection};

use std::ffi::CString;
use std::fmt::Write;

use x11rb::protocol::xproto::{self, ConnectionExt as _};

impl XConnection {
    /// "Request" a new activation token from the server.
    pub(crate) fn request_activation_token(
        &self,
        window_title: &str,
    ) -> Result<String, X11Error> {
        // The specification recommends the format "hostname+pid+"_TIME"+current time"
        let uname = rustix::system::uname();
        let pid = rustix::process::getpid();
        let time = self.timestamp();

        let activation_token = format!(
            "{}{}_TIME{}",
            uname.nodename().to_str().unwrap_or("winit"),
            pid.as_raw_nonzero(),
            time
        );

        // Set up the new startup notification.
        let notification = {
            let mut buffer = Vec::new();
            buffer.extend_from_slice(b"new: ID=");
            quote_string(&activation_token, &mut buffer);
            buffer.extend_from_slice(b" NAME=");
            quote_string(window_title, &mut buffer);
            buffer.extend_from_slice(b" SCREEN=");
            push_display(&mut buffer, &self.default_screen_index());

            CString::new(buffer)
                .map_err(|err| X11Error::InvalidActivationToken(err.into_vec()))?
                .into_bytes_with_nul()
        };
        self.send_message(&notification)?;

        Ok(activation_token)
    }

    /// Finish launching a window with the given startup ID.
    pub(crate) fn remove_activation_token(
        &self,
        window: xproto::Window,
        startup_id: &str,
    ) -> Result<(), X11Error> {
        let atoms = self.atoms();

        // Set the _NET_STARTUP_ID property on the window.
        self.xcb_connection()
            .change_property(
                xproto::PropMode::REPLACE,
                window,
                atoms[_NET_STARTUP_ID],
                xproto::AtomEnum::STRING,
                8,
                startup_id.len().try_into().unwrap(),
                startup_id.as_bytes(),
            )?
            .check()?;

        // Send the message indicating that the startup is over.
        let message = {
            const MESSAGE_ROOT: &str = "remove: ID=";

            let mut buffer = Vec::with_capacity(
                MESSAGE_ROOT
                    .len()
                    .checked_add(startup_id.len())
                    .and_then(|x| x.checked_add(1))
                    .unwrap(),
            );
            buffer.extend_from_slice(MESSAGE_ROOT.as_bytes());
            quote_string(startup_id, &mut buffer);
            CString::new(buffer)
                .map_err(|err| X11Error::InvalidActivationToken(err.into_vec()))?
                .into_bytes_with_nul()
        };

        self.send_message(&message)
    }

    /// Send a startup notification message to the window manager.
    fn send_message(&self, message: &[u8]) -> Result<(), X11Error> {
        let atoms = self.atoms();

        // Create a new window to send the message over.
        let screen = self.default_root();
        let window = xproto::WindowWrapper::create_window(
            self.xcb_connection(),
            screen.root_depth,
            screen.root,
            -100,
            -100,
            1,
            1,
            0,
            xproto::WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &xproto::CreateWindowAux::new()
                .override_redirect(1)
                .event_mask(
                    xproto::EventMask::STRUCTURE_NOTIFY
                        | xproto::EventMask::PROPERTY_CHANGE,
                ),
        )?;

        // Serialize the messages in 20-byte chunks.
        let mut message_type = atoms[_NET_STARTUP_INFO_BEGIN];
        message
            .chunks(20)
            .map(|chunk| {
                let mut buffer = [0u8; 20];
                buffer[..chunk.len()].copy_from_slice(chunk);
                let event = xproto::ClientMessageEvent::new(
                    8,
                    window.window(),
                    message_type,
                    buffer,
                );

                // Set the message type to the continuation atom for the next chunk.
                message_type = atoms[_NET_STARTUP_INFO];

                event
            })
            .try_for_each(|event| {
                // Send each event in order.
                self.xcb_connection()
                    .send_event(
                        false,
                        screen.root,
                        xproto::EventMask::PROPERTY_CHANGE,
                        event,
                    )
                    .map(VoidCookie::ignore_error)
            })?;

        Ok(())
    }
}

/// Quote a literal string as per the startup notification specification.
fn quote_string(s: &str, target: &mut Vec<u8>) {
    let total_len = s.len().checked_add(3).expect("quote string overflow");
    target.reserve(total_len);

    // Add the opening quote.
    target.push(b'"');

    // Iterate over the string split by literal quotes.
    s.as_bytes().split(|&b| b == b'"').for_each(|part| {
        // Add the part.
        target.extend_from_slice(part);

        // Escape the quote.
        target.push(b'\\');
        target.push(b'"');
    });

    // Un-escape the last quote.
    target.remove(target.len() - 2);
}

/// Push a `Display` implementation to the buffer.
fn push_display(buffer: &mut Vec<u8>, display: &impl std::fmt::Display) {
    struct Writer<'a> {
        buffer: &'a mut Vec<u8>,
    }

    impl<'a> std::fmt::Write for Writer<'a> {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            self.buffer.extend_from_slice(s.as_bytes());
            Ok(())
        }
    }

    write!(Writer { buffer }, "{display}").unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn properly_escapes_x11_messages() {
        let assert_eq = |input: &str, output: &[u8]| {
            let mut buf = vec![];
            quote_string(input, &mut buf);
            assert_eq!(buf, output);
        };

        assert_eq("", b"\"\"");
        assert_eq("foo", b"\"foo\"");
        assert_eq("foo\"bar", b"\"foo\\\"bar\"");
    }
}
