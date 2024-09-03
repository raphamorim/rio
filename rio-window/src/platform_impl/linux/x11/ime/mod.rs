// Important: all XIM calls need to happen from the same thread!

mod callbacks;
mod context;
mod inner;
mod input_method;

use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use tracing::debug;

use super::{ffi, util, XConnection, XError};

use self::callbacks::*;
use self::context::ImeContext;
pub use self::context::ImeContextCreationError;
use self::inner::{close_im, ImeInner};
use self::input_method::{PotentialInputMethods, Style};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImeEvent {
    Enabled,
    Start,
    Update(String, usize),
    End,
    Disabled,
}

pub type ImeReceiver = Receiver<ImeRequest>;
pub type ImeSender = Sender<ImeRequest>;
pub type ImeEventReceiver = Receiver<(ffi::Window, ImeEvent)>;
pub type ImeEventSender = Sender<(ffi::Window, ImeEvent)>;

/// Request to control XIM handler from the window.
pub enum ImeRequest {
    /// Set IME spot position for given `window_id`.
    Position(ffi::Window, i16, i16),

    /// Allow IME input for the given `window_id`.
    Allow(ffi::Window, bool),
}

#[derive(Debug)]
pub(crate) enum ImeCreationError {
    // Boxed to prevent large error type
    OpenFailure(Box<PotentialInputMethods>),
    SetDestroyCallbackFailed(#[allow(dead_code)] XError),
}

pub(crate) struct Ime {
    xconn: Arc<XConnection>,
    // The actual meat of this struct is boxed away, since it needs to have a fixed location in
    // memory so we can pass a pointer to it around.
    inner: Box<ImeInner>,
}

impl Ime {
    pub fn new(
        xconn: Arc<XConnection>,
        event_sender: ImeEventSender,
    ) -> Result<Self, ImeCreationError> {
        let potential_input_methods = PotentialInputMethods::new(&xconn);

        let (mut inner, client_data) = {
            let mut inner =
                Box::new(ImeInner::new(xconn, potential_input_methods, event_sender));
            let inner_ptr = Box::into_raw(inner);
            let client_data = inner_ptr as _;
            let destroy_callback = ffi::XIMCallback {
                client_data,
                callback: Some(xim_destroy_callback),
            };
            inner = unsafe { Box::from_raw(inner_ptr) };
            inner.destroy_callback = destroy_callback;
            (inner, client_data)
        };

        let xconn = Arc::clone(&inner.xconn);

        let input_method = inner.potential_input_methods.open_im(
            &xconn,
            Some(&|| {
                let _ = unsafe { set_instantiate_callback(&xconn, client_data) };
            }),
        );

        let is_fallback = input_method.is_fallback();
        if let Some(input_method) = input_method.ok() {
            inner.is_fallback = is_fallback;
            unsafe {
                let result = set_destroy_callback(&xconn, input_method.im, &inner)
                    .map_err(ImeCreationError::SetDestroyCallbackFailed);
                if result.is_err() {
                    let _ = close_im(&xconn, input_method.im);
                }
                result?;
            }
            inner.im = Some(input_method);
            Ok(Ime { xconn, inner })
        } else {
            Err(ImeCreationError::OpenFailure(Box::new(
                inner.potential_input_methods,
            )))
        }
    }

    pub fn is_destroyed(&self) -> bool {
        self.inner.is_destroyed
    }

    // This pattern is used for various methods here:
    // Ok(_) indicates that nothing went wrong internally
    // Ok(true) indicates that the action was actually performed
    // Ok(false) indicates that the action is not presently applicable
    pub fn create_context(
        &mut self,
        window: ffi::Window,
        with_preedit: bool,
    ) -> Result<bool, ImeContextCreationError> {
        let context = if self.is_destroyed() {
            // Create empty entry in map, so that when IME is rebuilt, this window has a context.
            None
        } else {
            let im = self.inner.im.as_ref().unwrap();
            let style = if with_preedit {
                im.preedit_style
            } else {
                im.none_style
            };

            let context = unsafe {
                ImeContext::new(
                    &self.inner.xconn,
                    im.im,
                    style,
                    window,
                    None,
                    self.inner.event_sender.clone(),
                )?
            };

            // Check the state on the context, since it could fail to enable or disable preedit.
            let event = if matches!(style, Style::None(_)) {
                if with_preedit {
                    debug!("failed to create IME context with preedit support.")
                }
                ImeEvent::Disabled
            } else {
                if !with_preedit {
                    debug!("failed to create IME context without preedit support.")
                }
                ImeEvent::Enabled
            };

            self.inner
                .event_sender
                .send((window, event))
                .expect("Failed to send enabled event");

            Some(context)
        };

        self.inner.contexts.insert(window, context);
        Ok(!self.is_destroyed())
    }

    pub fn get_context(&self, window: ffi::Window) -> Option<ffi::XIC> {
        if self.is_destroyed() {
            return None;
        }
        if let Some(Some(context)) = self.inner.contexts.get(&window) {
            Some(context.ic)
        } else {
            None
        }
    }

    pub fn remove_context(&mut self, window: ffi::Window) -> Result<bool, XError> {
        if let Some(Some(context)) = self.inner.contexts.remove(&window) {
            unsafe {
                self.inner.destroy_ic_if_necessary(context.ic)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn focus(&mut self, window: ffi::Window) -> Result<bool, XError> {
        if self.is_destroyed() {
            return Ok(false);
        }
        if let Some(&mut Some(ref mut context)) = self.inner.contexts.get_mut(&window) {
            context.focus(&self.xconn).map(|_| true)
        } else {
            Ok(false)
        }
    }

    pub fn unfocus(&mut self, window: ffi::Window) -> Result<bool, XError> {
        if self.is_destroyed() {
            return Ok(false);
        }
        if let Some(&mut Some(ref mut context)) = self.inner.contexts.get_mut(&window) {
            context.unfocus(&self.xconn).map(|_| true)
        } else {
            Ok(false)
        }
    }

    pub fn send_xim_spot(&mut self, window: ffi::Window, x: i16, y: i16) {
        if self.is_destroyed() {
            return;
        }
        if let Some(&mut Some(ref mut context)) = self.inner.contexts.get_mut(&window) {
            context.set_spot(&self.xconn, x as _, y as _);
        }
    }

    pub fn set_ime_allowed(&mut self, window: ffi::Window, allowed: bool) {
        if self.is_destroyed() {
            return;
        }

        if let Some(&mut Some(ref mut context)) = self.inner.contexts.get_mut(&window) {
            if allowed == context.is_allowed() {
                return;
            }
        }

        // Remove context for that window.
        let _ = self.remove_context(window);

        // Create new context supporting IME input.
        let _ = self.create_context(window, allowed);
    }
}

impl Drop for Ime {
    fn drop(&mut self) {
        unsafe {
            let _ = self.inner.destroy_all_contexts_if_necessary();
            let _ = self.inner.close_im_if_necessary();
        }
    }
}
