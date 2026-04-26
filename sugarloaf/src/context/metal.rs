use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};
use ::objc_rs::runtime::Object;
use ::objc_rs::{msg_send, sel, sel_impl};
use core_graphics::color_space::{kCGColorSpaceDisplayP3, CGColorSpace};
use core_graphics_types::geometry::CGSize;
use metal::foreign_types::ForeignType;
use metal::{
    CommandBuffer, CommandQueue, Device, MTLGPUFamily, MTLPixelFormat, MetalDrawable,
    MetalLayer, RenderCommandEncoder, Texture,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

pub struct MetalContext {
    pub device: Device,
    pub command_queue: CommandQueue,
    pub layer: MetalLayer,
    pub size: SugarloafWindowSize,
    pub scale: f32,
    pub supports_f16: bool,
}

pub struct MetalTexture {
    pub texture: Texture,
    pub drawable: MetalDrawable,
}

pub struct MetalCommandEncoder {
    pub command_buffer: CommandBuffer,
}

pub struct MetalRenderPass<'a> {
    pub encoder: RenderCommandEncoder,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl MetalContext {
    // type Device = Device;
    // type Queue = CommandQueue;
    // type Surface = MetalLayer;
    // type TextureFormat = MTLPixelFormat;
    // type CommandEncoder = MetalCommandEncoder;
    // type RenderPass<'a> = MetalRenderPass<'a>;
    // type Texture = MetalTexture;
    // type TextureView = Texture;
    // type Buffer = Buffer;
    // type BindGroup = (); // Metal doesn't have bind groups, we'll use argument buffers
    // type RenderPipeline = RenderPipelineState;
    // type ComputePipeline = ComputePipelineState;
    // type Sampler = SamplerState;

    pub fn new(sugarloaf_window: SugarloafWindow) -> Self {
        let device = Device::system_default().expect("Failed to create Metal device");
        let command_queue = device.new_command_queue();

        let size = sugarloaf_window.size;
        let scale = sugarloaf_window.scale;

        // Create Metal layer.
        //
        // Plain `BGRA8Unorm` + DisplayP3 colorspace tag — matches ghostty's
        // default `alpha-blending = native` on macOS (see
        // `ghostty/src/renderer/Metal.zig:208-211` and `Target.zig:44-60`).
        // No `_sRGB` suffix: we don't want Metal to apply a transfer curve
        // on read/write, so alpha blending stays in gamma-encoded space —
        // which is what Apple's native widgets do and what keeps text
        // weight identical to Terminal.app / ghostty. The fragment shaders
        // compensate by emitting *already* sRGB-encoded DisplayP3 values
        // (`prepare_output_rgb`), and the clear color goes through the
        // same encode chain on the Rust side (see `sugarloaf.rs`).
        //
        // The DisplayP3 colorspace tag controls which *primaries* stored
        // values represent on the display; we pick it unconditionally
        // because it's the widest gamut we can present without HDR. The
        // `[window] colorspace` config controls how input values are
        // *interpreted* (sRGB vs P3 vs Rec.2020) via the gamut-conversion
        // matrix in `renderer.metal`.
        let layer = MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm);
        // Triple buffering: allow 3 drawables in flight so CPU/GPU/compositor
        // can pipeline. `next_drawable` is the natural backpressure — it
        // blocks the main thread once 3 drawables are out, capping how far
        // ahead the CPU can run. Default is 2; ghostty/zed both bump this
        // to 3 to match the standard Apple sample pattern.
        layer.set_maximum_drawable_count(3);
        // Disable the 1-second wait timeout on `next_drawable` — under load
        // the default behaviour is to time out and return nil, which would
        // turn into a dropped frame. Matches zed's
        // `gpui_macos/src/metal_renderer.rs:162`.
        unsafe {
            let layer_obj = layer.as_ptr() as *mut Object;
            let _: () = msg_send![layer_obj, setAllowsNextDrawableTimeout: false];
        }
        // CAMetalLayer's `colorspace` setter isn't wrapped by the `metal`
        // crate yet, so we go through the Obj-C runtime directly. The CG
        // setter retains internally (standard CA property behaviour), but
        // we also `mem::forget` our owning handle to be fully defensive —
        // we create exactly one colorspace at startup, so a one-time leak
        // here is harmless and avoids a dangling pointer if Apple ever
        // changes the retention semantics.
        if let Some(cs) =
            CGColorSpace::create_with_name(unsafe { kCGColorSpaceDisplayP3 })
        {
            unsafe {
                let layer_obj = layer.as_ptr() as *mut Object;
                let cs_ptr = cs.as_ptr() as *mut Object;
                let _: () = msg_send![layer_obj, setColorspace: cs_ptr];
                let applied: *mut Object = msg_send![layer_obj, colorspace];
                if applied.is_null() {
                    tracing::warn!(
                        "CAMetalLayer.colorspace setter returned null — \
                         rendering will stay in the default sRGB colorspace"
                    );
                } else {
                    tracing::info!("CAMetalLayer colorspace set to Display P3");
                }
                std::mem::forget(cs);
            }
        } else {
            tracing::warn!("Failed to create Display P3 CGColorSpace");
        }
        // Preserve the alpha channel so macOS can actually composite
        // transparent Rio windows instead of forcing the layer opaque.
        layer.set_opaque(false);
        layer.set_presents_with_transaction(false);

        // Use CGSize from core_graphics_types
        let drawable_size = CGSize {
            width: size.width as f64,
            height: size.height as f64,
        };
        layer.set_drawable_size(drawable_size);
        layer.set_contents_scale(scale as f64);

        // Attach layer to window
        unsafe {
            match sugarloaf_window.window_handle().unwrap().as_raw() {
                RawWindowHandle::AppKit(handle) => {
                    let ns_view = handle.ns_view.as_ptr() as *mut Object;
                    let _: () = msg_send![ns_view, setLayer: layer.as_ref()];
                    let _: () = msg_send![ns_view, setWantsLayer: true];
                }
                _ => panic!("Metal backend only supports macOS AppKit windows"),
            }
        }

        // Check for f16 support (Metal generally supports it on modern hardware)
        let supports_f16 = device.supports_family(MTLGPUFamily::Apple1);

        tracing::info!("Metal device created: {:?}", device.name());

        MetalContext {
            device,
            command_queue,
            layer,
            size: SugarloafWindowSize {
                width: size.width,
                height: size.height,
            },
            scale,
            supports_f16,
        }
    }

    #[inline]
    pub fn resize(&mut self, width: u32, height: u32) {
        self.size.width = width as f32;
        self.size.height = height as f32;
        let drawable_size = CGSize {
            width: width as f64,
            height: height as f64,
        };
        self.layer.set_drawable_size(drawable_size);
    }

    #[inline]
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    #[inline]
    pub fn get_current_texture(&self) -> Result<MetalTexture, String> {
        if let Some(drawable) = self.layer.next_drawable() {
            let texture = drawable.texture().to_owned();
            Ok(MetalTexture {
                texture,
                drawable: drawable.to_owned(),
            })
        } else {
            Err("Failed to get next drawable".to_string())
        }
    }

    // fn create_command_encoder(&self) -> Self::CommandEncoder {
    // let command_buffer = self.command_queue.new_command_buffer().to_owned();
    // MetalCommandEncoder { command_buffer }
    // }

    pub fn supports_f16(&self) -> bool {
        self.supports_f16
    }
}
