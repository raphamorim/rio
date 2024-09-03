use wasm_bindgen::prelude::Closure;
use wasm_bindgen::JsCast;
use web_sys::MediaQueryList;

pub(super) struct MediaQueryListHandle {
    mql: MediaQueryList,
    closure: Closure<dyn FnMut()>,
}

impl MediaQueryListHandle {
    pub fn new<F>(window: &web_sys::Window, media_query: &str, mut listener: F) -> Self
    where
        F: 'static + FnMut(&MediaQueryList),
    {
        let mql = window
            .match_media(media_query)
            .expect("Failed to parse media query")
            .expect("Found empty media query");

        let closure = Closure::new({
            let mql = mql.clone();
            move || listener(&mql)
        });
        // TODO: Replace obsolete `addListener()` with `addEventListener()` and use
        // `MediaQueryListEvent` instead of cloning the `MediaQueryList`.
        // Requires Safari v14.
        mql.add_listener_with_opt_callback(Some(closure.as_ref().unchecked_ref()))
            .expect("Invalid listener");

        Self { mql, closure }
    }

    pub fn mql(&self) -> &MediaQueryList {
        &self.mql
    }
}

impl Drop for MediaQueryListHandle {
    fn drop(&mut self) {
        remove_listener(&self.mql, &self.closure);
    }
}

fn remove_listener(mql: &MediaQueryList, listener: &Closure<dyn FnMut()>) {
    mql.remove_listener_with_opt_callback(Some(listener.as_ref().unchecked_ref()))
        .unwrap_or_else(|e| {
            web_sys::console::error_2(&"Error removing media query listener".into(), &e)
        });
}
