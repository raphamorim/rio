extern crate tokio;

use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use renderer::components::rect::Rect;
use renderer::renderer::{Renderable, CustomRenderer, RendererTarget};

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Rect example")
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let mut renderer = CustomRenderer::new(
        RendererTarget::Desktop,
        &window,
        wgpu::PowerPreference::HighPerformance,
    )
    .await;
    let mut rect = Rect::init(renderer.get_context());
    renderer.add_component(&mut rect);

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                // renderer.add_component(&mut rect);
                // window.request_redraw();
            }
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => control_flow.set_exit(),
                WindowEvent::KeyboardInput {
                    input:
                        KeyboardInput {
                            virtual_keycode: Some(VirtualKeyCode::Space),
                            state: ElementState::Released,
                            ..
                        },
                    ..
                } => {
                    //
                }
                _ => (),
            },
            Event::RedrawRequested { .. } => {
                renderer.render();
            }
            _ => {
                *control_flow = winit::event_loop::ControlFlow::Wait;
            }
        }
    });
}
