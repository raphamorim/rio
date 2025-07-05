use sugarloaf::components::quad::Quad;
use sugarloaf::Sugarloaf;

fn main() {
    // Create a simple example showing frosted glass blur quads
    let mut sugarloaf = Sugarloaf::new(
        winit::window::Window::new(&winit::event_loop::EventLoop::new().unwrap()).unwrap(),
        wgpu::PowerPreference::HighPerformance,
        None,
    ).unwrap();

    // Create some frosted glass quads with different intensities
    let quads = vec![
        // Background quad (no blur)
        Quad::solid([50.0, 50.0], [500.0, 400.0], [0.1, 0.1, 0.2, 1.0]),
        
        // Raycast-style interface with strong frosted glass effect
        Quad::blur([100.0, 100.0], [400.0, 300.0], [0.12, 0.16, 0.21, 0.6], 20.0)
            .with_border([0.3, 0.3, 0.3, 0.3], [12.0, 12.0, 12.0, 12.0], 1.0),
        
        // Light frosted glass panel
        Quad::blur([150.0, 150.0], [200.0, 100.0], [0.8, 0.8, 0.9, 0.4], 5.0),
        
        // Medium frosted glass with rounded corners
        Quad::blur([300.0, 200.0], [150.0, 120.0], [0.2, 0.8, 0.3, 0.5], 10.0)
            .with_border([0.0, 1.0, 0.0, 0.8], [20.0, 20.0, 20.0, 20.0], 2.0),
        
        // Strong frosted glass effect
        Quad::blur([200.0, 350.0], [180.0, 80.0], [0.9, 0.7, 0.1, 0.7], 25.0),
    ];

    // Add quads to sugarloaf state
    for quad in quads {
        sugarloaf.pile(quad);
    }

    println!("Frosted glass blur example created!");
    println!("This implementation creates a frosted glass effect using procedural noise.");
    println!("Blur intensities used: 5.0, 10.0, 20.0, 25.0");
    println!("Perfect for Raycast-style interfaces and modern UI designs!");
}