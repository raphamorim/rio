mod animation_frame;
mod canvas;
pub mod event;
mod event_handle;
mod fullscreen;
mod intersection_handle;
mod media_query_handle;
mod pointer;
mod resize_scaling;
mod schedule;

pub use self::canvas::{Canvas, Style};
pub use self::event::ButtonsState;
pub use self::event_handle::EventListenerHandle;
pub use self::resize_scaling::ResizeScaleHandle;
pub use self::schedule::Schedule;

use crate::dpi::{LogicalPosition, LogicalSize};
use wasm_bindgen::closure::Closure;
use web_sys::{Document, HtmlCanvasElement, PageTransitionEvent, VisibilityState};

pub fn throw(msg: &str) {
    wasm_bindgen::throw_str(msg);
}

pub struct PageTransitionEventHandle {
    _show_listener: event_handle::EventListenerHandle<dyn FnMut(PageTransitionEvent)>,
    _hide_listener: event_handle::EventListenerHandle<dyn FnMut(PageTransitionEvent)>,
}

pub fn on_page_transition(
    window: web_sys::Window,
    show_handler: impl FnMut(PageTransitionEvent) + 'static,
    hide_handler: impl FnMut(PageTransitionEvent) + 'static,
) -> PageTransitionEventHandle {
    let show_closure = Closure::new(show_handler);
    let hide_closure = Closure::new(hide_handler);

    let show_listener =
        event_handle::EventListenerHandle::new(window.clone(), "pageshow", show_closure);
    let hide_listener =
        event_handle::EventListenerHandle::new(window, "pagehide", hide_closure);
    PageTransitionEventHandle {
        _show_listener: show_listener,
        _hide_listener: hide_listener,
    }
}

pub fn scale_factor(window: &web_sys::Window) -> f64 {
    window.device_pixel_ratio()
}

fn fix_canvas_size(style: &Style, mut size: LogicalSize<f64>) -> LogicalSize<f64> {
    if style.get("box-sizing") == "border-box" {
        size.width += style_size_property(style, "border-left-width")
            + style_size_property(style, "border-right-width")
            + style_size_property(style, "padding-left")
            + style_size_property(style, "padding-right");
        size.height += style_size_property(style, "border-top-width")
            + style_size_property(style, "border-bottom-width")
            + style_size_property(style, "padding-top")
            + style_size_property(style, "padding-bottom");
    }

    size
}

pub fn set_canvas_size(
    document: &Document,
    raw: &HtmlCanvasElement,
    style: &Style,
    new_size: LogicalSize<f64>,
) {
    if !document.contains(Some(raw)) || style.get("display") == "none" {
        return;
    }

    let new_size = fix_canvas_size(style, new_size);

    style.set("width", &format!("{}px", new_size.width));
    style.set("height", &format!("{}px", new_size.height));
}

pub fn set_canvas_min_size(
    document: &Document,
    raw: &HtmlCanvasElement,
    style: &Style,
    dimensions: Option<LogicalSize<f64>>,
) {
    if let Some(dimensions) = dimensions {
        if !document.contains(Some(raw)) || style.get("display") == "none" {
            return;
        }

        let new_size = fix_canvas_size(style, dimensions);

        style.set("min-width", &format!("{}px", new_size.width));
        style.set("min-height", &format!("{}px", new_size.height));
    } else {
        style.remove("min-width");
        style.remove("min-height");
    }
}

pub fn set_canvas_max_size(
    document: &Document,
    raw: &HtmlCanvasElement,
    style: &Style,
    dimensions: Option<LogicalSize<f64>>,
) {
    if let Some(dimensions) = dimensions {
        if !document.contains(Some(raw)) || style.get("display") == "none" {
            return;
        }

        let new_size = fix_canvas_size(style, dimensions);

        style.set("max-width", &format!("{}px", new_size.width));
        style.set("max-height", &format!("{}px", new_size.height));
    } else {
        style.remove("max-width");
        style.remove("max-height");
    }
}

pub fn set_canvas_position(
    document: &Document,
    raw: &HtmlCanvasElement,
    style: &Style,
    mut position: LogicalPosition<f64>,
) {
    if document.contains(Some(raw)) && style.get("display") != "none" {
        position.x -= style_size_property(style, "margin-left")
            + style_size_property(style, "border-left-width")
            + style_size_property(style, "padding-left");
        position.y -= style_size_property(style, "margin-top")
            + style_size_property(style, "border-top-width")
            + style_size_property(style, "padding-top");
    }

    style.set("position", "fixed");
    style.set("left", &format!("{}px", position.x));
    style.set("top", &format!("{}px", position.y));
}

/// This function will panic if the element is not inserted in the DOM
/// or is not a CSS property that represents a size in pixel.
pub fn style_size_property(style: &Style, property: &str) -> f64 {
    let prop = style.get(property);
    prop.strip_suffix("px")
        .expect("Element was not inserted into the DOM or is not a size in pixel")
        .parse()
        .expect("CSS property is not a size in pixel")
}

pub fn is_dark_mode(window: &web_sys::Window) -> Option<bool> {
    window
        .match_media("(prefers-color-scheme: dark)")
        .ok()
        .flatten()
        .map(|media| media.matches())
}

pub fn is_visible(document: &Document) -> bool {
    document.visibility_state() == VisibilityState::Visible
}

pub type RawCanvasType = HtmlCanvasElement;
