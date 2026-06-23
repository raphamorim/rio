//! Minimal runtime binding to libcanberra, loaded via `dlopen` so Rio gains no
//! build-time dependency and degrades gracefully when the library is absent —
//! matching how the rest of the project loads Linux system libraries (x11-dl,
//! xkbcommon-dl, wayland-dlopen).
//!
//! Playing through libcanberra is what makes the bell respect the user's
//! freedesktop event-sound theme, output routing, volume, mute and
//! Do-Not-Disturb state.

use libloading::Library;
use std::ffi::{c_char, c_int, c_void, CString};
use std::sync::OnceLock;

type CaContext = c_void;
type CaProplist = c_void;

type CaContextCreate = unsafe extern "C" fn(*mut *mut CaContext) -> c_int;
type CaContextChangePropsFull =
    unsafe extern "C" fn(*mut CaContext, *mut CaProplist) -> c_int;
type CaContextPlayFull = unsafe extern "C" fn(
    *mut CaContext,
    u32,
    *mut CaProplist,
    *mut c_void,
    *mut c_void,
) -> c_int;
type CaProplistCreate = unsafe extern "C" fn(*mut *mut CaProplist) -> c_int;
type CaProplistSets =
    unsafe extern "C" fn(*mut CaProplist, *const c_char, *const c_char) -> c_int;
type CaProplistDestroy = unsafe extern "C" fn(*mut CaProplist) -> c_int;

struct Canberra {
    // Kept alive so the resolved function pointers remain valid.
    _lib: Library,
    context: *mut CaContext,
    play_full: CaContextPlayFull,
    proplist_create: CaProplistCreate,
    proplist_sets: CaProplistSets,
    proplist_destroy: CaProplistDestroy,
}

// The context is only ever touched from the main event-loop thread, but the
// `OnceLock` requires the stored value to be `Sync`. The raw pointer and
// function pointers are valid for the process lifetime.
unsafe impl Send for Canberra {}
unsafe impl Sync for Canberra {}

static CANBERRA: OnceLock<Option<Canberra>> = OnceLock::new();

impl Canberra {
    unsafe fn load() -> Option<Canberra> {
        let lib = Library::new("libcanberra.so.0")
            .or_else(|_| Library::new("libcanberra.so"))
            .ok()?;

        let create: CaContextCreate = *lib.get(b"ca_context_create\0").ok()?;
        let change_props_full: CaContextChangePropsFull =
            *lib.get(b"ca_context_change_props_full\0").ok()?;
        let play_full: CaContextPlayFull = *lib.get(b"ca_context_play_full\0").ok()?;
        let proplist_create: CaProplistCreate = *lib.get(b"ca_proplist_create\0").ok()?;
        let proplist_sets: CaProplistSets = *lib.get(b"ca_proplist_sets\0").ok()?;
        let proplist_destroy: CaProplistDestroy =
            *lib.get(b"ca_proplist_destroy\0").ok()?;

        let mut context: *mut CaContext = std::ptr::null_mut();
        let err = create(&mut context);
        if err != 0 || context.is_null() {
            tracing::debug!("ca_context_create failed (error {err})");
            return None;
        }

        // The PulseAudio/PipeWire backend refuses to open a context that has no
        // application name (driver_open returns CA_ERROR_STATE), which would
        // make every play silently fail. Set it up front.
        let mut props: *mut CaProplist = std::ptr::null_mut();
        if proplist_create(&mut props) == 0 && !props.is_null() {
            proplist_sets(props, c"application.name".as_ptr(), c"Rio".as_ptr());
            proplist_sets(props, c"application.id".as_ptr(), c"rio".as_ptr());
            proplist_sets(props, c"application.icon_name".as_ptr(), c"rio".as_ptr());
            change_props_full(context, props);
            proplist_destroy(props);
        }

        Some(Canberra {
            _lib: lib,
            context,
            play_full,
            proplist_create,
            proplist_sets,
            proplist_destroy,
        })
    }
}

fn canberra() -> Option<&'static Canberra> {
    CANBERRA
        .get_or_init(|| unsafe { Canberra::load() })
        .as_ref()
}

/// Play a freedesktop event sound by id (e.g. `"bell"`). Returns immediately;
/// libcanberra plays the sound asynchronously. No-op if libcanberra cannot be
/// loaded.
pub fn play(event_id: &str) {
    let Some(canberra) = canberra() else {
        tracing::debug!("libcanberra unavailable; skipping system bell sound");
        return;
    };
    let Ok(id) = CString::new(event_id) else {
        return;
    };

    unsafe {
        let mut proplist: *mut CaProplist = std::ptr::null_mut();
        if (canberra.proplist_create)(&mut proplist) != 0 || proplist.is_null() {
            return;
        }
        // CA_PROP_EVENT_ID
        (canberra.proplist_sets)(proplist, c"event.id".as_ptr(), id.as_ptr());
        let err = (canberra.play_full)(
            canberra.context,
            0,
            proplist,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        (canberra.proplist_destroy)(proplist);

        if err != 0 {
            tracing::warn!("libcanberra failed to play '{event_id}' (error {err})");
        }
    }
}
