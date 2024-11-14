//! Handling of xdg activation, which is used for user attention requests.

use std::sync::atomic::AtomicBool;
use std::sync::Weak;

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};
use sctk::reexports::protocols::xdg::activation::v1::client::xdg_activation_token_v1::{
    Event as ActivationTokenEvent, XdgActivationTokenV1,
};
use sctk::reexports::protocols::xdg::activation::v1::client::xdg_activation_v1::XdgActivationV1;

use sctk::globals::GlobalData;

use crate::event_loop::AsyncRequestSerial;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::WindowId;
use crate::window::ActivationToken;

pub struct XdgActivationState {
    xdg_activation: XdgActivationV1,
}

impl XdgActivationState {
    pub fn bind(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let xdg_activation = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { xdg_activation })
    }

    pub fn global(&self) -> &XdgActivationV1 {
        &self.xdg_activation
    }
}

impl Dispatch<XdgActivationV1, GlobalData, WinitState> for XdgActivationState {
    fn event(
        _state: &mut WinitState,
        _proxy: &XdgActivationV1,
        _event: <XdgActivationV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<XdgActivationTokenV1, XdgActivationTokenData, WinitState>
    for XdgActivationState
{
    fn event(
        state: &mut WinitState,
        proxy: &XdgActivationTokenV1,
        event: <XdgActivationTokenV1 as Proxy>::Event,
        data: &XdgActivationTokenData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        let token = match event {
            ActivationTokenEvent::Done { token } => token,
            _ => return,
        };

        let global = state
            .xdg_activation
            .as_ref()
            .expect("got xdg_activation event without global.")
            .global();

        match data {
            XdgActivationTokenData::Attention((surface, fence)) => {
                global.activate(token, surface);
                // Mark that no request attention is in process.
                if let Some(attention_requested) = fence.upgrade() {
                    attention_requested
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                }
            }
            XdgActivationTokenData::Obtain((window_id, serial)) => {
                state.events_sink.push_window_event(
                    crate::event::WindowEvent::ActivationTokenDone {
                        serial: *serial,
                        token: ActivationToken::_new(token),
                    },
                    *window_id,
                );
            }
        }

        proxy.destroy();
    }
}

/// The data associated with the activation request.
pub enum XdgActivationTokenData {
    /// Request user attention for the given surface.
    Attention((WlSurface, Weak<AtomicBool>)),
    /// Get a token to be passed outside of the winit.
    Obtain((WindowId, AsyncRequestSerial)),
}

delegate_dispatch!(WinitState: [ XdgActivationV1: GlobalData] => XdgActivationState);
delegate_dispatch!(WinitState: [ XdgActivationTokenV1: XdgActivationTokenData] => XdgActivationState);
