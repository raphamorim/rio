//! Relative pointer.

use std::ops::Deref;

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::{delegate_dispatch, Dispatch};
use sctk::reexports::client::{Connection, QueueHandle};
use sctk::reexports::protocols::wp::relative_pointer::zv1::{
    client::zwp_relative_pointer_manager_v1::ZwpRelativePointerManagerV1,
    client::zwp_relative_pointer_v1::{self, ZwpRelativePointerV1},
};

use sctk::globals::GlobalData;

use crate::event::DeviceEvent;
use crate::platform_impl::wayland::state::WinitState;

/// Wrapper around the relative pointer.
pub struct RelativePointerState {
    manager: ZwpRelativePointerManagerV1,
}

impl RelativePointerState {
    /// Create new relative pointer manager.
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }
}

impl Deref for RelativePointerState {
    type Target = ZwpRelativePointerManagerV1;

    fn deref(&self) -> &Self::Target {
        &self.manager
    }
}

impl Dispatch<ZwpRelativePointerManagerV1, GlobalData, WinitState>
    for RelativePointerState
{
    fn event(
        _state: &mut WinitState,
        _proxy: &ZwpRelativePointerManagerV1,
        _event: <ZwpRelativePointerManagerV1 as wayland_client::Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ZwpRelativePointerV1, GlobalData, WinitState> for RelativePointerState {
    fn event(
        state: &mut WinitState,
        _proxy: &ZwpRelativePointerV1,
        event: <ZwpRelativePointerV1 as wayland_client::Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<WinitState>,
    ) {
        let (dx_unaccel, dy_unaccel) = match event {
            zwp_relative_pointer_v1::Event::RelativeMotion {
                dx_unaccel,
                dy_unaccel,
                ..
            } => (dx_unaccel, dy_unaccel),
            _ => return,
        };
        state.events_sink.push_device_event(
            DeviceEvent::Motion {
                axis: 0,
                value: dx_unaccel,
            },
            super::DeviceId,
        );
        state.events_sink.push_device_event(
            DeviceEvent::Motion {
                axis: 1,
                value: dy_unaccel,
            },
            super::DeviceId,
        );
        state.events_sink.push_device_event(
            DeviceEvent::MouseMotion {
                delta: (dx_unaccel, dy_unaccel),
            },
            super::DeviceId,
        );
    }
}

delegate_dispatch!(WinitState: [ZwpRelativePointerV1: GlobalData] => RelativePointerState);
delegate_dispatch!(WinitState: [ZwpRelativePointerManagerV1: GlobalData] => RelativePointerState);
