//! Handling of KDE-compatible blur.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur::OrgKdeKwinBlur;
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager;

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

/// KWin blur manager.
#[derive(Debug, Clone)]
pub struct KWinBlurManager {
    manager: OrgKdeKwinBlurManager,
}

impl KWinBlurManager {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn blur(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<WinitState>,
    ) -> OrgKdeKwinBlur {
        self.manager.create(surface, queue_handle, ())
    }

    pub fn unset(&self, surface: &WlSurface) {
        self.manager.unset(surface)
    }
}

impl Dispatch<OrgKdeKwinBlurManager, GlobalData, WinitState> for KWinBlurManager {
    fn event(
        _: &mut WinitState,
        _: &OrgKdeKwinBlurManager,
        _: <OrgKdeKwinBlurManager as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for org_kde_kwin_blur_manager");
    }
}

impl Dispatch<OrgKdeKwinBlur, (), WinitState> for KWinBlurManager {
    fn event(
        _: &mut WinitState,
        _: &OrgKdeKwinBlur,
        _: <OrgKdeKwinBlur as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for org_kde_kwin_blur");
    }
}

delegate_dispatch!(WinitState: [OrgKdeKwinBlurManager: GlobalData] => KWinBlurManager);
delegate_dispatch!(WinitState: [OrgKdeKwinBlur: ()] => KWinBlurManager);
