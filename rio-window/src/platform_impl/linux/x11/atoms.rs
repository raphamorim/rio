//! Collects every atom used by the platform implementation.

use core::ops::Index;

macro_rules! atom_manager {
    ($($name:ident $(:$lit:literal)?),*) => {
        x11rb::atom_manager! {
            /// The atoms used by `winit`
            pub Atoms: AtomsCookie {
                $($name $(:$lit)?,)*
            }
        }

        /// Indices into the `Atoms` struct.
        #[derive(Copy, Clone, Debug)]
        #[allow(non_camel_case_types)]
        pub enum AtomName {
            $($name,)*
        }

        impl AtomName {
            pub(crate) fn atom_from(
                self,
                atoms: &Atoms
            ) -> &x11rb::protocol::xproto::Atom {
                match self {
                    $(AtomName::$name => &atoms.$name,)*
                }
            }
        }
    };
}

atom_manager! {
    // General Use Atoms
    CARD32,
    UTF8_STRING,
    WM_CHANGE_STATE,
    WM_CLIENT_MACHINE,
    WM_DELETE_WINDOW,
    WM_PROTOCOLS,
    WM_STATE,
    XIM_SERVERS,

    // Assorted ICCCM Atoms
    _NET_WM_ICON,
    _NET_WM_MOVERESIZE,
    _NET_WM_NAME,
    _NET_WM_PID,
    _NET_WM_PING,
    _NET_WM_STATE,
    _NET_WM_STATE_ABOVE,
    _NET_WM_STATE_BELOW,
    _NET_WM_STATE_FULLSCREEN,
    _NET_WM_STATE_HIDDEN,
    _NET_WM_STATE_MAXIMIZED_HORZ,
    _NET_WM_STATE_MAXIMIZED_VERT,
    _NET_WM_WINDOW_TYPE,

    // Activation atoms.
    _NET_STARTUP_INFO_BEGIN,
    _NET_STARTUP_INFO,
    _NET_STARTUP_ID,

    // WM window types.
    _NET_WM_WINDOW_TYPE_DESKTOP,
    _NET_WM_WINDOW_TYPE_DOCK,
    _NET_WM_WINDOW_TYPE_TOOLBAR,
    _NET_WM_WINDOW_TYPE_MENU,
    _NET_WM_WINDOW_TYPE_UTILITY,
    _NET_WM_WINDOW_TYPE_SPLASH,
    _NET_WM_WINDOW_TYPE_DIALOG,
    _NET_WM_WINDOW_TYPE_DROPDOWN_MENU,
    _NET_WM_WINDOW_TYPE_POPUP_MENU,
    _NET_WM_WINDOW_TYPE_TOOLTIP,
    _NET_WM_WINDOW_TYPE_NOTIFICATION,
    _NET_WM_WINDOW_TYPE_COMBO,
    _NET_WM_WINDOW_TYPE_DND,
    _NET_WM_WINDOW_TYPE_NORMAL,

    // Drag-N-Drop Atoms
    XdndAware,
    XdndEnter,
    XdndLeave,
    XdndDrop,
    XdndPosition,
    XdndStatus,
    XdndActionPrivate,
    XdndSelection,
    XdndFinished,
    XdndTypeList,
    TextUriList: b"text/uri-list",
    None: b"None",

    // Miscellaneous Atoms
    _GTK_THEME_VARIANT,
    _MOTIF_WM_HINTS,
    _NET_ACTIVE_WINDOW,
    _NET_CLIENT_LIST,
    _NET_FRAME_EXTENTS,
    _NET_SUPPORTED,
    _NET_SUPPORTING_WM_CHECK,
    _XEMBED,
    _XSETTINGS_SETTINGS
}

impl Index<AtomName> for Atoms {
    type Output = x11rb::protocol::xproto::Atom;

    fn index(&self, index: AtomName) -> &Self::Output {
        index.atom_from(self)
    }
}

pub(crate) use AtomName::*;
// Make sure `None` is still defined.
pub(crate) use core::option::Option::None;
