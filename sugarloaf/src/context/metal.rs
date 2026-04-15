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
        // `BGRA8Unorm_sRGB` + DisplayP3 colorspace matches ghostty's Metal
        // setup (see `src/renderer/metal/Target.zig`). The `_sRGB` suffix
        // tells Metal to sRGB-encode on write and decode on read, so the
        // alpha blending stage operates in linear light — eliminates the
        // "dark halo" / muddy-edge artifact that gamma-incorrect blending
        // produces around text and translucent overlays. DisplayP3 widens
        // the gamut ~26% past sRGB primaries, so configured theme colors
        // land closer to Apple Terminal / ghostty's vivid look. Requires
        // the `renderer.metal` fragment shaders to output linear RGB (see
        // `srgb_to_linear`), otherwise the HW encode would brighten every
        // pixel.
        let layer = MetalLayer::new();
        layer.set_device(&device);
        layer.set_pixel_format(MTLPixelFormat::BGRA8Unorm_sRGB);
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
    //     let command_buffer = self.command_queue.new_command_buffer().to_owned();
    //     MetalCommandEncoder { command_buffer }
    // }

    pub fn supports_f16(&self) -> bool {
        self.supports_f16
    }
}
