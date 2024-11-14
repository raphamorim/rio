#![cfg(target_arch = "wasm32")]

use winit::{
    event::{ElementState, Event, KeyEvent, WindowEvent},
    event_loop::EventLoop,
    keyboard::Key,
    window::{Fullscreen, WindowBuilder},
};

pub fn main() -> Result<(), impl std::error::Error> {
    let event_loop = EventLoop::new().unwrap();

    let builder = WindowBuilder::new().with_title("Rio Wasm");
    #[cfg(target_arch = "wasm32")]
    let builder = {
        use winit::platform::web::WindowBuilderExtWebSys;
        builder.with_append(true)
    };
    let window = builder.build(&event_loop).unwrap();

    #[cfg(target_arch = "wasm32")]
    let log_list = wasm::insert_canvas_and_create_log_list(&window);

    event_loop.run(move |event, elwt| {
        #[cfg(target_arch = "wasm32")]
        wasm::log_event(&log_list, &event);

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => elwt.exit(),
            Event::AboutToWait => {
                window.request_redraw();
            }
            Event::WindowEvent {
                window_id,
                event:
                    WindowEvent::KeyboardInput {
                        event:
                            KeyEvent {
                                logical_key: Key::Character(c),
                                state: ElementState::Released,
                                ..
                            },
                        ..
                    },
            } if window_id == window.id() && c == "f" => {
                if window.fullscreen().is_some() {
                    window.set_fullscreen(None);
                } else {
                    window.set_fullscreen(Some(Fullscreen::Borderless(None)));
                }
            }
            _ => (),
        }
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::num::NonZeroU32;

    use softbuffer::{Surface, SurfaceExtWeb};
    use winit::{
        event::{Event, WindowEvent},
        window::Window,
    };
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(start)]
    pub fn run() {
        console_tracing::init_with_level(tracing::Level::Debug)
            .expect("error initializing logger");

        #[allow(clippy::main_recursion)]
        let _ = super::main();
    }

    pub fn insert_canvas_and_create_log_list(window: &Window) -> web_sys::Element {
        use winit::platform::web::WindowExtWebSys;

        let canvas = window.canvas().unwrap();
        let mut surface = Surface::from_canvas(canvas.clone()).unwrap();
        surface
            .resize(
                NonZeroU32::new(canvas.width()).unwrap(),
                NonZeroU32::new(canvas.height()).unwrap(),
            )
            .unwrap();
        let mut buffer = surface.buffer_mut().unwrap();
        buffer.fill(0xFFF0000);
        buffer.present().unwrap();

        let window = web_sys::window().unwrap();
        let document = window.document().unwrap();
        let body = document.body().unwrap();

        let style = &canvas.style();
        style.set_property("margin", "50px").unwrap();
        // Use to test interactions with border and padding.
        //style.set_property("border", "50px solid black").unwrap();
        //style.set_property("padding", "50px").unwrap();

        let log_header = document.create_element("h2").unwrap();
        log_header.set_text_content(Some("Event Log"));
        body.append_child(&log_header).unwrap();

        let log_list = document.create_element("ul").unwrap();
        body.append_child(&log_list).unwrap();
        log_list
    }

    pub fn log_event(log_list: &web_sys::Element, event: &Event<()>) {
        tracing::debug!("{:?}", event);

        // Getting access to browser logs requires a lot of setup on mobile devices.
        // So we implement this basic logging system into the page to give developers an easy alternative.
        // As a bonus its also kind of handy on desktop.
        let event = match event {
            Event::WindowEvent {
                event: WindowEvent::RedrawRequested,
                ..
            } => None,
            Event::WindowEvent { event, .. } => Some(format!("{event:?}")),
            Event::Resumed | Event::Suspended => Some(format!("{event:?}")),
            _ => None,
        };
        if let Some(event) = event {
            let window = web_sys::window().unwrap();
            let document = window.document().unwrap();
            let log = document.create_element("li").unwrap();

            let date = js_sys::Date::new_0();
            log.set_text_content(Some(&format!(
                "{:02}:{:02}:{:02}.{:03}: {event}",
                date.get_hours(),
                date.get_minutes(),
                date.get_seconds(),
                date.get_milliseconds(),
            )));

            log_list
                .insert_before(&log, log_list.first_child().as_ref())
                .unwrap();
        }
    }
}
