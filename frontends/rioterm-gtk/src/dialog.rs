//! A yes/no question, on GTK 4.6's message dialog.

use gtk::prelude::*;

#[allow(deprecated)]
pub fn confirm(
    parent: Option<&gtk::Window>,
    heading: &str,
    body: &str,
    accept: &str,
    done: impl Fn(bool) + 'static,
) {
    let dialog = gtk::MessageDialog::builder()
        .modal(true)
        .message_type(gtk::MessageType::Warning)
        .text(heading)
        .secondary_text(body)
        .build();
    dialog.set_transient_for(parent);
    dialog.add_button("Cancel", gtk::ResponseType::Cancel);
    let button = dialog.add_button(accept, gtk::ResponseType::Accept);
    button.add_css_class("destructive-action");
    dialog.set_default_response(gtk::ResponseType::Cancel);
    dialog.connect_response(move |dialog, response| {
        dialog.close();
        done(response == gtk::ResponseType::Accept);
    });
    dialog.present();
}
