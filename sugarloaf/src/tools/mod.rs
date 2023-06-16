pub mod counter;

#[cfg(target_arch = "wasm32")]
pub const CANVAS_ELEMENT_ID: &str = "sugarloaf-canvas";

#[cfg(target_arch = "wasm32")]
pub fn create_html_canvas() -> web_sys::HtmlCanvasElement {
    use wasm_bindgen::JsCast;

    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let body = doc.body().unwrap();
            let canvas = doc.create_element("canvas").unwrap();
            canvas.set_attribute("data-raw-handle", "1").unwrap();
            canvas.set_id(CANVAS_ELEMENT_ID);
            body.append_child(&canvas).unwrap();
            canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok()
        })
        .expect("couldn't append canvas to document body")
}

#[cfg(target_arch = "wasm32")]
pub fn get_html_canvas() -> web_sys::HtmlCanvasElement {
    use wasm_bindgen::JsCast;
    use web_sys::HtmlCanvasElement;

    let canvas_element = {
        std::panic::set_hook(Box::new(console_error_panic_hook::hook));

        web_sys::window()
            .and_then(|win| win.document())
            .and_then(|doc| doc.get_element_by_id(CANVAS_ELEMENT_ID))
            .and_then(|element| element.dyn_into::<HtmlCanvasElement>().ok())
            .expect("Get canvas element")
    };

    canvas_element
}
