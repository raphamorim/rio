//! Handling of the KDE application menu protocol.
//!
//! The protocol carries no menu: it tells the compositor where on the
//! session bus the `com.canonical.dbusmenu` object of a window is, so the
//! desktop's menu bar can show it while the window is active.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};
use wayland_protocols_plasma::appmenu::client::org_kde_kwin_appmenu::OrgKdeKwinAppmenu;
use wayland_protocols_plasma::appmenu::client::org_kde_kwin_appmenu_manager::OrgKdeKwinAppmenuManager;

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

/// KDE application menu manager.
#[derive(Debug, Clone)]
pub struct KdeAppMenuManager {
    manager: OrgKdeKwinAppmenuManager,
}

impl KdeAppMenuManager {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn app_menu(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<WinitState>,
    ) -> OrgKdeKwinAppmenu {
        self.manager.create(surface, queue_handle, ())
    }
}

impl Dispatch<OrgKdeKwinAppmenuManager, GlobalData, WinitState> for KdeAppMenuManager {
    fn event(
        _: &mut WinitState,
        _: &OrgKdeKwinAppmenuManager,
        _: <OrgKdeKwinAppmenuManager as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for org_kde_kwin_appmenu_manager");
    }
}

impl Dispatch<OrgKdeKwinAppmenu, (), WinitState> for KdeAppMenuManager {
    fn event(
        _: &mut WinitState,
        _: &OrgKdeKwinAppmenu,
        _: <OrgKdeKwinAppmenu as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for org_kde_kwin_appmenu");
    }
}

delegate_dispatch!(WinitState: [OrgKdeKwinAppmenuManager: GlobalData] => KdeAppMenuManager);
delegate_dispatch!(WinitState: [OrgKdeKwinAppmenu: ()] => KdeAppMenuManager);
