//! Handling of background effect.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::ExtBackgroundEffectManagerV1;
use sctk::reexports::protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

/// Wayland background effect manager.
#[derive(Debug, Clone)]
pub struct BackgroundEffectManager {
    manager: ExtBackgroundEffectManagerV1,
}

impl BackgroundEffectManager {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn background_effect(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<WinitState>,
    ) -> ExtBackgroundEffectSurfaceV1 {
        self.manager
            .get_background_effect(surface, queue_handle, ())
    }
}

impl Dispatch<ExtBackgroundEffectManagerV1, GlobalData, WinitState>
    for BackgroundEffectManager
{
    fn event(
        _: &mut WinitState,
        _: &ExtBackgroundEffectManagerV1,
        _: <ExtBackgroundEffectManagerV1 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for wayland_background_effect_manager");
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, (), WinitState> for BackgroundEffectManager {
    fn event(
        _: &mut WinitState,
        _: &ExtBackgroundEffectSurfaceV1,
        _: <ExtBackgroundEffectSurfaceV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for wayland_background_effect_surface");
    }
}

delegate_dispatch!(WinitState: [ExtBackgroundEffectManagerV1: GlobalData] => BackgroundEffectManager);
delegate_dispatch!(WinitState: [ExtBackgroundEffectSurfaceV1: ()] => BackgroundEffectManager);
