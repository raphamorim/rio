pub mod counter;

#[cfg(all(target_arch = "wasm32"))]
pub fn create_html_canvas() -> web_sys::HtmlCanvasElement {
    use wasm_bindgen::JsCast;

    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let body = doc.body().unwrap();
            let canvas = doc.create_element("canvas").unwrap();
            canvas.set_attribute("data-raw-handle", "1").unwrap();
            canvas.set_id("sugarloaf-canvas");
            body.append_child(&canvas).unwrap();
            canvas.dyn_into::<web_sys::HtmlCanvasElement>().ok()
        })
        .expect("couldn't append canvas to document body")
}