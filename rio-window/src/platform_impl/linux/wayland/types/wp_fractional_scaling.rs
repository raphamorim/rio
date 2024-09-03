//! Handling of the fractional scaling.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle};
use sctk::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use sctk::reexports::protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::{
    Event as FractionalScalingEvent, WpFractionalScaleV1,
};

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

/// The scaling factor denominator.
const SCALE_DENOMINATOR: f64 = 120.;

/// Fractional scaling manager.
#[derive(Debug)]
pub struct FractionalScalingManager {
    manager: WpFractionalScaleManagerV1,
}

pub struct FractionalScaling {
    /// The surface used for scaling.
    surface: WlSurface,
}

impl FractionalScalingManager {
    /// Create new viewporter.
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, 1..=1, GlobalData)?;
        Ok(Self { manager })
    }

    pub fn fractional_scaling(
        &self,
        surface: &WlSurface,
        queue_handle: &QueueHandle<WinitState>,
    ) -> WpFractionalScaleV1 {
        let data = FractionalScaling {
            surface: surface.clone(),
        };
        self.manager
            .get_fractional_scale(surface, queue_handle, data)
    }
}

impl Dispatch<WpFractionalScaleManagerV1, GlobalData, WinitState>
    for FractionalScalingManager
{
    fn event(
        _: &mut WinitState,
        _: &WpFractionalScaleManagerV1,
        _: <WpFractionalScaleManagerV1 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        // No events.
    }
}

impl Dispatch<WpFractionalScaleV1, FractionalScaling, WinitState>
    for FractionalScalingManager
{
    fn event(
        state: &mut WinitState,
        _: &WpFractionalScaleV1,
        event: <WpFractionalScaleV1 as Proxy>::Event,
        data: &FractionalScaling,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        if let FractionalScalingEvent::PreferredScale { scale } = event {
            state.scale_factor_changed(
                &data.surface,
                scale as f64 / SCALE_DENOMINATOR,
                false,
            );
        }
    }
}

delegate_dispatch!(WinitState: [WpFractionalScaleManagerV1: GlobalData] => FractionalScalingManager);
delegate_dispatch!(WinitState: [WpFractionalScaleV1: FractionalScaling] => FractionalScalingManager);
