extern crate tokio;

use winit::platform::run_return::EventLoopExtRunReturn;
use winit::{
    dpi::LogicalSize,
    event::{ElementState, Event, KeyboardInput, VirtualKeyCode, WindowEvent},
    event_loop::EventLoop,
    window::WindowBuilder,
};

use sugarloaf::components::row::Row;
use sugarloaf::{CustomRenderer, Renderable, RendererTarget};

#[tokio::main]
async fn main() {
    let mut event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Row example")
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
    let mut row = Row::init(renderer.get_context());
    renderer.add_component(&mut row);

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
                WindowEvent::ScaleFactorChanged {
                    new_inner_size,
                    scale_factor,
                    ..
                } => {
                    renderer
                        .rescale(scale_factor as f32)
                        .resize(new_inner_size.width, new_inner_size.height)
                        .render();
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
