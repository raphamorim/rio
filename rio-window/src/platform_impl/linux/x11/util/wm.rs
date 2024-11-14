use std::sync::Mutex;

use super::*;

// https://specifications.freedesktop.org/wm-spec/latest/ar01s04.html#idm46075117309248
pub const MOVERESIZE_TOPLEFT: isize = 0;
pub const MOVERESIZE_TOP: isize = 1;
pub const MOVERESIZE_TOPRIGHT: isize = 2;
pub const MOVERESIZE_RIGHT: isize = 3;
pub const MOVERESIZE_BOTTOMRIGHT: isize = 4;
pub const MOVERESIZE_BOTTOM: isize = 5;
pub const MOVERESIZE_BOTTOMLEFT: isize = 6;
pub const MOVERESIZE_LEFT: isize = 7;
pub const MOVERESIZE_MOVE: isize = 8;

// This info is global to the window manager.
static SUPPORTED_HINTS: Mutex<Vec<xproto::Atom>> = Mutex::new(Vec::new());
static WM_NAME: Mutex<Option<String>> = Mutex::new(None);

pub fn hint_is_supported(hint: xproto::Atom) -> bool {
    (*SUPPORTED_HINTS.lock().unwrap()).contains(&hint)
}

pub fn wm_name_is_one_of(names: &[&str]) -> bool {
    if let Some(ref name) = *WM_NAME.lock().unwrap() {
        names.contains(&name.as_str())
    } else {
        false
    }
}

impl XConnection {
    pub fn update_cached_wm_info(&self, root: xproto::Window) {
        *SUPPORTED_HINTS.lock().unwrap() = self.get_supported_hints(root);
        *WM_NAME.lock().unwrap() = self.get_wm_name(root);
    }

    fn get_supported_hints(&self, root: xproto::Window) -> Vec<xproto::Atom> {
        let atoms = self.atoms();
        let supported_atom = atoms[_NET_SUPPORTED];
        self.get_property(
            root,
            supported_atom,
            xproto::Atom::from(xproto::AtomEnum::ATOM),
        )
        .unwrap_or_else(|_| Vec::with_capacity(0))
    }

    #[allow(clippy::useless_conversion)]
    fn get_wm_name(&self, root: xproto::Window) -> Option<String> {
        let atoms = self.atoms();
        let check_atom = atoms[_NET_SUPPORTING_WM_CHECK];
        let wm_name_atom = atoms[_NET_WM_NAME];

        // Mutter/Muffin/Budgie doesn't have _NET_SUPPORTING_WM_CHECK in its _NET_SUPPORTED, despite
        // it working and being supported. This has been reported upstream, but due to the
        // inavailability of time machines, we'll just try to get _NET_SUPPORTING_WM_CHECK
        // regardless of whether or not the WM claims to support it.
        //
        // Blackbox 0.70 also incorrectly reports not supporting this, though that appears to be
        // fixed in 0.72.
        // if !supported_hints.contains(&check_atom) {
        // return None;
        // }

        // IceWM (1.3.x and earlier) doesn't report supporting _NET_WM_NAME, but will nonetheless
        // provide us with a value for it. Note that the unofficial 1.4 fork of IceWM works fine.
        // if !supported_hints.contains(&wm_name_atom) {
        // return None;
        // }

        // Of the WMs tested, only xmonad and dwm fail to provide a WM name.

        // Querying this property on the root window will give us the ID of a child window created
        // by the WM.
        let root_window_wm_check = {
            let result = self.get_property::<xproto::Window>(
                root,
                check_atom,
                xproto::Atom::from(xproto::AtomEnum::WINDOW),
            );

            let wm_check = result.ok().and_then(|wm_check| wm_check.first().cloned());

            wm_check?
        };

        // Querying the same property on the child window we were given, we should get this child
        // window's ID again.
        let child_window_wm_check = {
            let result = self.get_property::<xproto::Window>(
                root_window_wm_check.into(),
                check_atom,
                xproto::Atom::from(xproto::AtomEnum::WINDOW),
            );

            let wm_check = result.ok().and_then(|wm_check| wm_check.first().cloned());

            wm_check?
        };

        // These values should be the same.
        if root_window_wm_check != child_window_wm_check {
            return None;
        }

        // All of that work gives us a window ID that we can get the WM name from.
        let wm_name = {
            let atoms = self.atoms();
            let utf8_string_atom = atoms[UTF8_STRING];

            let result = self.get_property(
                root_window_wm_check.into(),
                wm_name_atom,
                utf8_string_atom,
            );

            // IceWM requires this. IceWM was also the only WM tested that returns a null-terminated
            // string. For more fun trivia, IceWM is also unique in including version and uname
            // information in this string (this means you'll have to be careful if you want to match
            // against it, though).
            // The unofficial 1.4 fork of IceWM still includes the extra details, but properly
            // returns a UTF8 string that isn't null-terminated.
            let no_utf8 = if let Err(ref err) = result {
                err.is_actual_property_type(xproto::Atom::from(xproto::AtomEnum::STRING))
            } else {
                false
            };

            if no_utf8 {
                self.get_property(
                    root_window_wm_check.into(),
                    wm_name_atom,
                    xproto::Atom::from(xproto::AtomEnum::STRING),
                )
            } else {
                result
            }
        }
        .ok();

        wm_name.and_then(|wm_name| String::from_utf8(wm_name).ok())
    }
}
