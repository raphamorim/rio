// mod bar;
mod shared;
mod term;
mod window;

use std::path::PathBuf;
use crate::term::Term;
use config::Config;
use std::error::Error;
use winit::{event, event_loop};

fn terminfo_exists(terminfo: &str) -> bool {
    // Get first terminfo character for the parent directory.
    let first = terminfo.get(..1).unwrap_or_default();
    let first_hex = format!("{:x}", first.chars().next().unwrap_or_default() as usize);

    // Return true if the terminfo file exists at the specified location.
    macro_rules! check_path {
        ($path:expr) => {
            if $path.join(first).join(terminfo).exists()
                || $path.join(&first_hex).join(terminfo).exists()
            {
                return true;
            }
        };
    }

    if let Some(dir) = std::env::var_os("TERMINFO") {
        check_path!(PathBuf::from(&dir));
    } else if let Some(home) = dirs::home_dir() {
        check_path!(home.join(".terminfo"));
    }

    if let Ok(dirs) = std::env::var("TERMINFO_DIRS") {
        for dir in dirs.split(':') {
            check_path!(PathBuf::from(dir));
        }
    }

    if let Ok(prefix) = std::env::var("PREFIX") {
        let path = PathBuf::from(prefix);
        check_path!(path.join("etc/terminfo"));
        check_path!(path.join("lib/terminfo"));
        check_path!(path.join("share/terminfo"));
    }

    check_path!(PathBuf::from("/etc/terminfo"));
    check_path!(PathBuf::from("/lib/terminfo"));
    check_path!(PathBuf::from("/usr/share/terminfo"));
    check_path!(PathBuf::from("/boot/system/data/terminfo"));

    // No valid terminfo path has been found.
    false
}

pub fn setup_env(config: &Config) {
    // Default to 'alacritty' terminfo if it is available, otherwise
    // default to 'xterm-256color'. May be overridden by user's config
    // below.
    let terminfo = if terminfo_exists("rio") { "rio" } else { "xterm-256color" };
    std::env::set_var("TERM", terminfo);

    // Advertise 24-bit color support.
    std::env::set_var("COLORTERM", "truecolor");

    // Prevent child processes from inheriting startup notification env.
    std::env::remove_var("DESKTOP_STARTUP_ID");

    // Set env vars from config.
    // for (key, value) in config.env.iter() {
    //     std::env::set_var(key, value);
    // }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = Config::load_macos();
    let event_loop = event_loop::EventLoopBuilder::new().build();
    let window_builder =
        window::create_window_builder("Rio", (config.width, config.height));
    let winit_window = window_builder.build(&event_loop).unwrap();

    setup_env(&config);

    let mut input_stream = window::input::Input::new();
    let mut rio = Term::new(&winit_window, config).await?;
    let mut is_focused = true;
    event_loop.run(move |event, _, control_flow| {
        match event {
            event::Event::WindowEvent {
                event: event::WindowEvent::CloseRequested,
                ..
            } => *control_flow = event_loop::ControlFlow::Exit,

            event::Event::WindowEvent {
                event: event::WindowEvent::ModifiersChanged(modifiers),
                ..
            } => input_stream.set_modifiers(modifiers),

            // event::Event::WindowEvent {
            //     event: event::WindowEvent::MouseWheel { delta, .. },
            //     ..
            // } => {
            //     let mut scroll_y: f64 = 0.0;
            //     match delta {
            //         winit::event::MouseScrollDelta::LineDelta(_x, _y) => {
            //             // scroll_y = y;
            //         }

            //         winit::event::MouseScrollDelta::PixelDelta(pixel_delta) => {
            //             scroll_y = pixel_delta.y;
            //         }
            //     }

            //     // hacky
            //     if scroll_y < 0.0 {
            //         rio.set_text_scroll(-3.0_f32);
            //         // winit_window.request_redraw();
            //     }
            //     if scroll_y > 0.0 {
            //         rio.set_text_scroll(3.0_f32);
            //     }
            // }
            event::Event::WindowEvent {
                event: event::WindowEvent::ReceivedCharacter(character),
                ..
            } => {
                // println!("character: {:?}", character);
                // input_stream.input_character(character, &mut rio.write_process);
            }

            event::Event::WindowEvent {
                event:
                    event::WindowEvent::KeyboardInput {
                        input:
                            winit::event::KeyboardInput {
                                // semantic meaning of the key
                                virtual_keycode,
                                // physical key pressed
                                scancode,
                                state,
                                // modifiers,
                                ..
                            },
                        ..
                    },
                ..
            } => match state {
                winit::event::ElementState::Pressed => {
                    // println!("{:?} {:?}", scancode, Some(virtual_keycode));
                    // input_stream.keydown(
                    //     scancode,
                    //     virtual_keycode,
                    //     &mut rio.write_process,
                    // );
                    winit_window.request_redraw();
                }

                winit::event::ElementState::Released => {
                    winit_window.request_redraw();
                }
            },

            event::Event::WindowEvent {
                event: event::WindowEvent::Focused(focused),
                ..
            } => {
                is_focused = focused;
            }

            event::Event::WindowEvent {
                event: event::WindowEvent::Resized(new_size),
                ..
            } => {
                rio.set_size(new_size);
                winit_window.request_redraw();
            }

            event::Event::WindowEvent {
                event:
                    event::WindowEvent::ScaleFactorChanged {
                        new_inner_size,
                        scale_factor,
                    },
                ..
            } => {
                let scale_factor_f32 = scale_factor as f32;
                // if rio.get_scale() != scale_factor_f32 {
                rio.set_scale(scale_factor_f32, *new_inner_size);
                // }
            }

            event::Event::MainEventsCleared { .. } => {
                winit_window.request_redraw();
            }

            event::Event::RedrawRequested { .. } => {
                if rio.renderer.config.advanced.disable_render_when_unfocused
                    && is_focused
                {
                    return;
                }

                rio.draw();
            }
            _ => {
                let next_frame_time =
                    std::time::Instant::now() + std::time::Duration::from_nanos(500_000);
                *control_flow = event_loop::ControlFlow::WaitUntil(next_frame_time);
                // *control_flow = event_loop::ControlFlow::Wait;
            }
        }
    })
}
