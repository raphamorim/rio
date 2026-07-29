//! Handling of ext-background-effect-v1 protocol.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

/// ext-background-effect-v1 background effect manager.
#[derive(Debug, Clone)]
pub struct ExtBackgroundEffectManager {
    manager: ExtBackgroundEffectManagerV1,
}

impl ExtBackgroundEffectManager {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn get_background_effect(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<WinitState>,
    ) -> ExtBackgroundEffectSurfaceV1 {
        self.manager
            .get_background_effect(surface, queue_handle, ())
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, GlobalData, WinitState>
    for ExtBackgroundEffectManager
{
    fn event(
        _state: &mut WinitState,
        _proxy: &ExtBackgroundEffectManagerV1,
        _event: <ExtBackgroundEffectManagerV1 as Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qh: &QueueHandle<WinitState>,
    ) {
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, (), WinitState>
    for ExtBackgroundEffectManager
{
    fn event(
        _state: &mut WinitState,
        _proxy: &ExtBackgroundEffectSurfaceV1,
        _event: <ExtBackgroundEffectSurfaceV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for ext_background_effect_surface_v1");
    }
}

delegate_dispatch!(WinitState: [ExtBackgroundEffectManagerV1: GlobalData] => ExtBackgroundEffectManager);
delegate_dispatch!(WinitState: [ExtBackgroundEffectSurfaceV1: ()] => ExtBackgroundEffectManager);
