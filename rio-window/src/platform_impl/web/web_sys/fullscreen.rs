use std::cell::OnceCell;

use js_sys::Promise;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::wasm_bindgen;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Element, HtmlCanvasElement};

pub fn request_fullscreen(document: &Document, canvas: &HtmlCanvasElement) {
    if is_fullscreen(document, canvas) {
        return;
    }

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(extends = HtmlCanvasElement)]
        type RequestFullscreen;

        #[wasm_bindgen(method, js_name = requestFullscreen)]
        fn request_fullscreen(this: &RequestFullscreen) -> Promise;

        #[wasm_bindgen(method, js_name = webkitRequestFullscreen)]
        fn webkit_request_fullscreen(this: &RequestFullscreen);
    }

    let canvas: &RequestFullscreen = canvas.unchecked_ref();

    if has_fullscreen_api_support(canvas) {
        thread_local! {
            static REJECT_HANDLER: Closure<dyn FnMut(JsValue)> = Closure::new(|_| ());
        }
        REJECT_HANDLER.with(|handler| {
            let _ = canvas.request_fullscreen().catch(handler);
        });
    } else {
        canvas.webkit_request_fullscreen();
    }
}

pub fn is_fullscreen(document: &Document, canvas: &HtmlCanvasElement) -> bool {
    #[wasm_bindgen]
    extern "C" {
        type FullscreenElement;

        #[wasm_bindgen(method, getter, js_name = webkitFullscreenElement)]
        fn webkit_fullscreen_element(this: &FullscreenElement) -> Option<Element>;
    }

    let element = if has_fullscreen_api_support(canvas) {
        #[allow(clippy::disallowed_methods)]
        document.fullscreen_element()
    } else {
        let document: &FullscreenElement = document.unchecked_ref();
        document.webkit_fullscreen_element()
    };

    match element {
        Some(element) => {
            let canvas: &Element = canvas;
            canvas == &element
        }
        None => false,
    }
}

pub fn exit_fullscreen(document: &Document, canvas: &HtmlCanvasElement) {
    #[wasm_bindgen]
    extern "C" {
        type ExitFullscreen;

        #[wasm_bindgen(method, js_name = webkitExitFullscreen)]
        fn webkit_exit_fullscreen(this: &ExitFullscreen);
    }

    if has_fullscreen_api_support(canvas) {
        #[allow(clippy::disallowed_methods)]
        document.exit_fullscreen()
    } else {
        let document: &ExitFullscreen = document.unchecked_ref();
        document.webkit_exit_fullscreen()
    }
}

fn has_fullscreen_api_support(canvas: &HtmlCanvasElement) -> bool {
    thread_local! {
        static FULLSCREEN_API_SUPPORT: OnceCell<bool> = const { OnceCell::new() };
    }

    FULLSCREEN_API_SUPPORT.with(|support| {
        *support.get_or_init(|| {
            #[wasm_bindgen]
            extern "C" {
                type CanvasFullScreenApiSupport;

                #[wasm_bindgen(method, getter, js_name = requestFullscreen)]
                fn has_request_fullscreen(this: &CanvasFullScreenApiSupport) -> JsValue;
            }

            let support: &CanvasFullScreenApiSupport = canvas.unchecked_ref();
            !support.has_request_fullscreen().is_undefined()
        })
    })
}
