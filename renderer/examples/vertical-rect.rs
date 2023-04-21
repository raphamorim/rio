extern crate tokio;

use winit::platform::run_return::EventLoopExtRunReturn;
use renderer::renderable::Renderable;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use renderer::renderer::{ Renderer, RendererTarget };
use renderer::components::vertical_rect::VerticalRect;

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("VerticalRect example")
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .with_resizable(true)
        .build(&event_loop)
        .unwrap();

    let mut renderer = Renderer::new(RendererTarget::Desktop, &window, wgpu::PowerPreference::HighPerformance).await;
    let mut vertical_rect = VerticalRect::init(renderer.get_context());
    renderer.add_component(&mut vertical_rect);

    event_loop.run_return(move |event, _, control_flow| {
        control_flow.set_wait();

        match event {
            Event::Resumed => {
                // renderer.add_component(&mut rect);
                // window.request_redraw();
            },
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
                },
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
