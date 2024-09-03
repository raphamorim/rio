//! Handling of the wp-viewporter.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};
use sctk::reexports::protocols::wp::viewporter::client::wp_viewport::WpViewport;
use sctk::reexports::protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

/// Viewporter.
#[derive(Debug)]
pub struct ViewporterState {
    viewporter: WpViewporter,
}

impl ViewporterState {
    /// Create new viewporter.
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let viewporter = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { viewporter })
    }

    /// Get the viewport for the given object.
    pub fn get_viewport(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<WinitState>,
    ) -> WpViewport {
        self.viewporter
            .get_viewport(surface, queue_handle, GlobalData)
    }
}

impl Dispatch<WpViewporter, GlobalData, WinitState> for ViewporterState {
    fn event(
        _: &mut WinitState,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        // No events.
    }
}
impl Dispatch<WpViewport, GlobalData, WinitState> for ViewporterState {
    fn event(
        _: &mut WinitState,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        // No events.
    }
}

delegate_dispatch!(WinitState: [WpViewporter: GlobalData] => ViewporterState);
delegate_dispatch!(WinitState: [WpViewport: GlobalData] => ViewporterState);
