//! Handling of the cupertino-app-effects-v1 protocol.
//!
//! Only the title bar style is used. With a unified style the server keeps
//! decorating the window but draws the window control buttons alone, over
//! the top of the surface: the row they lie in is the client's to draw and
//! to move the window from.

use sctk::reexports::client::globals::{BindError, GlobalList};
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{
    delegate_dispatch, Connection, Dispatch, Proxy, QueueHandle,
};

use sctk::globals::GlobalData;

use crate::platform_impl::wayland::state::WinitState;

use self::protocol::cupertino_app_effects_v1::{CupertinoAppEffectsV1, TitlebarStyle};

// The scanner macros read the XML without telling rustc, so name the file
// here to rebuild the bindings when it changes.
const _: &[u8] = include_bytes!("../../../../../protocols/cupertino-app-effects-v1.xml");

#[allow(dead_code, non_camel_case_types, unused_unsafe, unused_variables)]
#[allow(non_upper_case_globals, non_snake_case, unused_imports)]
#[allow(missing_docs, clippy::all)]
mod protocol {
    use sctk::reexports::client as wayland_client;
    use wayland_client::protocol::*;

    pub mod __interfaces {
        use sctk::reexports::client as wayland_client;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/cupertino-app-effects-v1.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("protocols/cupertino-app-effects-v1.xml");
}

/// The version that added `unified_compact`: a 40 pt row with the buttons
/// centred 20 pt from the top, the closest to the height of Rio's tab strip.
/// The `unified` style of version 1 expects a 52 pt row.
const VERSION: u32 = 6;

/// The `cupertino_app_effects_v1` global.
#[derive(Debug, Clone)]
pub struct CupertinoAppEffects {
    manager: CupertinoAppEffectsV1,
}

impl CupertinoAppEffects {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
    ) -> Result<Self, BindError> {
        let manager = globals.bind(queue_handle, VERSION..=VERSION, GlobalData)?;
        Ok(Self { manager })
    }

    /// Ask the server to draw only the window control buttons of `surface`.
    ///
    /// The style is fixed once the window is mapped: the server refuses the
    /// request for a surface that has committed a buffer.
    pub fn set_unified_titlebar(&self, surface: &WlSurface) {
        self.manager
            .set_titlebar_style(surface, TitlebarStyle::UnifiedCompact);
    }
}

impl Dispatch<CupertinoAppEffectsV1, GlobalData, WinitState> for CupertinoAppEffects {
    fn event(
        _: &mut WinitState,
        _: &CupertinoAppEffectsV1,
        _: <CupertinoAppEffectsV1 as Proxy>::Event,
        _: &GlobalData,
        _: &Connection,
        _: &QueueHandle<WinitState>,
    ) {
        unreachable!("no events defined for cupertino_app_effects_v1");
    }
}

delegate_dispatch!(WinitState: [CupertinoAppEffectsV1: GlobalData] => CupertinoAppEffects);
