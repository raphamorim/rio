# Metal Backend Support

Sugarloaf now supports native Metal rendering on macOS as an alternative to WebGPU. This provides better performance and more direct access to Apple's graphics APIs.

## Features

- **Native Metal Context**: Direct Metal device and command queue initialization
- **Automatic Backend Selection**: Metal is used by default on macOS when the `native-metal` feature is enabled
- **Fallback Support**: Graceful fallback to WebGPU for rendering while Metal context is available
- **F16 Support**: Half-precision floating point support detection
- **Window Integration**: Proper Metal layer attachment to macOS windows

## Usage

### Basic Setup

```rust
use sugarloaf::{RenderBackend, SugarloafRenderer};

let renderer = SugarloafRenderer {
    render_backend: RenderBackend::Metal,
    ..Default::default()
};
```

### Feature Flags

Add to your `Cargo.toml`:

```toml
[dependencies]
sugarloaf = { version = "0.2.17", features = ["native-metal"] }
```

The `native-metal` feature is enabled by default and includes:
- `metal` - Metal API bindings
- `objc` - Objective-C runtime for macOS integration
- `core-graphics-types` - Core Graphics types for Metal integration

### Backend Detection

```rust
// Check if Metal backend is being used
if sugarloaf.is_using_metal() {
    println!("Using native Metal rendering");
}

// Get the current render backend
match sugarloaf.render_backend() {
    RenderBackend::Metal => println!("Metal backend active"),
    RenderBackend::WebGpu => println!("WebGPU backend active"),
}

// Access Metal context (when available)
#[cfg(feature = "native-metal")]
if let Some(metal_ctx) = sugarloaf.get_context().metal_context() {
    println!("Metal device: {:?}", metal_ctx.device.name());
    println!("F16 support: {}", metal_ctx.supports_f16);
}
```

## Implementation Status

### ✅ Completed
- Metal context initialization
- Device and command queue creation
- Surface and layer management
- Window integration (macOS AppKit)
- F16 support detection
- Backend selection and switching
- Basic Metal infrastructure
- **Metal text rendering support detection**
- **F16-optimized Metal shaders for text and quads**

### 🚧 In Progress
- Metal-specific render pipelines for quads
- Metal-specific render pipelines for text
- Complete Metal rendering implementation
- Shader compilation (MSL to Metal)

### 📋 Planned
- Metal Performance Shaders integration
- Advanced Metal features (argument buffers, etc.)
- Performance optimizations
- Metal debugging tools integration

## Platform Support

- **macOS**: Full Metal context support, rendering falls back to WebGPU
- **iOS/iPadOS**: Planned for future releases
- **Other platforms**: Falls back to WebGPU

## Current Behavior

When using the Metal backend:

1. **Metal Context**: A native Metal device and command queue are created
2. **Metal Layer**: A CAMetalLayer is attached to the window for rendering
3. **Rendering**: Currently falls back to WebGPU for actual rendering operations
4. **Performance**: Metal context is available for future optimizations

## Examples

### Basic Metal Backend Demo

See `examples/metal_backend_demo.rs` for a complete example showing:
- Metal backend initialization
- Backend switching between Metal and WebGPU
- Metal context inspection
- Device information display

```bash
cargo run --example metal_backend_demo --release
```

### Controls in Demo
- **SPACE**: Toggle between Metal and WebGPU backends
- **ESC**: Exit the demo

## Performance Benefits (Future)

When Metal rendering is fully implemented, you can expect:

- **Lower CPU overhead**: Direct Metal API calls without WebGPU abstraction
- **Better memory management**: Native Metal memory pools and heaps
- **Improved GPU utilization**: Access to Metal-specific optimizations
- **Reduced latency**: Fewer abstraction layers between your code and the GPU

## Troubleshooting

### Metal Context Available but WebGPU Rendering
This is expected behavior in the current implementation. The Metal context is initialized and available, but rendering operations currently fall back to WebGPU for compatibility.

### Metal Not Available
If Metal initialization fails, Sugarloaf will automatically fall back to WebGPU. Check the logs for initialization messages.

### Debugging
Enable debug logging to see backend selection:

```rust
// In your application
tracing::info!("Backend selection and Metal context creation");
```

Look for log messages like:
- "Metal device created"
- "Using Metal backend"
- "Metal F16 support: true/false"
- "Metal backend requested for QuadBrush, but falling back to WebGPU for now"

## Development Roadmap

### Phase 1: Foundation (✅ Complete)
- Metal context initialization
- Backend selection
- Window integration

### Phase 2: Basic Rendering (🚧 In Progress)
- Metal quad rendering
- Metal text rendering
- Shader compilation

### Phase 3: Advanced Features (📋 Planned)
- Performance optimizations
- Metal-specific features
- Complete WebGPU replacement