// Copyright (c) 2023-present, Raphael Amorim.
//
// CPU rendering backend context.
//
// The rasterizer writes premultiplied 0xAARRGGBB u32 values straight into
// the frame's buffer, with no intermediate pixmap or format conversion. The
// buffer is either a window's softbuffer surface, presented after each
// frame, or one the context owns (`new_offscreen`) for a host that shows the
// frame itself, such as a GTK widget handing it to GTK as a texture.

use crate::sugarloaf::{SugarloafWindow, SugarloafWindowSize};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle};
use std::num::NonZeroU32;
use std::rc::Rc;

pub struct SoftbufferHandle {
    window: RawWindowHandle,
    display: RawDisplayHandle,
}

impl raw_window_handle::HasWindowHandle for SoftbufferHandle {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(self.window) })
    }
}

impl raw_window_handle::HasDisplayHandle for SoftbufferHandle {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError>
    {
        Ok(unsafe { raw_window_handle::DisplayHandle::borrow_raw(self.display) })
    }
}

unsafe impl Send for SoftbufferHandle {}
unsafe impl Sync for SoftbufferHandle {}

pub type CpuSurface = softbuffer::Surface<Rc<SoftbufferHandle>, Rc<SoftbufferHandle>>;

/// Where a CPU frame is drawn.
pub enum CpuTarget {
    /// A window's softbuffer surface, presented after each frame.
    Window {
        surface: CpuSurface,
        _handle: Rc<SoftbufferHandle>,
    },
    /// A buffer the host reads after each frame (see
    /// [`CpuContext::frame`]): no window and no present.
    Offscreen {
        pixels: Vec<u32>,
        /// Counts the frames drawn into `pixels`, so the host can tell a
        /// new frame from one the renderer skipped as unchanged.
        generation: u64,
    },
}

/// The last frame drawn into an offscreen target: premultiplied
/// 0xAARRGGBB pixels, `width` per row, no padding.
pub struct CpuFrame<'a> {
    pub pixels: &'a [u32],
    pub width: u32,
    pub height: u32,
    pub generation: u64,
}

pub struct CpuContext {
    pub size: SugarloafWindowSize,
    pub scale: f32,
    /// Buffer width in u32 elements. Row stride equals this.
    pub width_px: u32,
    pub height_px: u32,
    pub target: CpuTarget,
}

impl CpuContext {
    pub fn new(window: SugarloafWindow) -> Self {
        let size = window.size;
        let scale = window.scale;

        let handle = Rc::new(SoftbufferHandle {
            window: window.handle,
            display: window.display,
        });

        let context = softbuffer::Context::new(handle.clone())
            .expect("CPU backend: failed to create softbuffer context");
        let mut surface = softbuffer::Surface::new(&context, handle.clone())
            .expect("CPU backend: failed to create softbuffer surface");

        let width = (size.width as u32).max(1);
        let height = (size.height as u32).max(1);

        if let (Some(w), Some(h)) = (NonZeroU32::new(width), NonZeroU32::new(height)) {
            surface
                .resize(w, h)
                .expect("CPU backend: failed to size softbuffer surface");
        }

        Self {
            size,
            scale,
            width_px: width,
            height_px: height,
            target: CpuTarget::Window {
                surface,
                _handle: handle,
            },
        }
    }

    /// A context drawing into a buffer it owns, for a host that shows the
    /// frame itself. `size` is in physical pixels.
    pub fn new_offscreen(size: SugarloafWindowSize, scale: f32) -> Self {
        let width = (size.width as u32).max(1);
        let height = (size.height as u32).max(1);
        Self {
            size,
            scale,
            width_px: width,
            height_px: height,
            target: CpuTarget::Offscreen {
                pixels: vec![0; width as usize * height as usize],
                generation: 0,
            },
        }
    }

    /// Whether the frame keeps per-pixel alpha, so `window.opacity`
    /// reaches whoever composites it: DWM on Windows (softbuffer's GDI
    /// path copies the alpha byte through), and any host of an offscreen
    /// frame. softbuffer's other presenters drop the byte (macOS
    /// `NoneSkipFirst`, Wayland `Xrgb8888`), so there the background stays
    /// opaque, as premultiplying would only darken it.
    pub fn keeps_alpha(&self) -> bool {
        match self.target {
            CpuTarget::Window { .. } => cfg!(target_os = "windows"),
            CpuTarget::Offscreen { .. } => true,
        }
    }

    /// The last offscreen frame, or `None` for a window target.
    pub fn frame(&self) -> Option<CpuFrame<'_>> {
        match &self.target {
            CpuTarget::Window { .. } => None,
            CpuTarget::Offscreen { pixels, generation } => Some(CpuFrame {
                pixels,
                width: self.width_px,
                height: self.height_px,
                generation: *generation,
            }),
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.size.width = width as f32;
        self.size.height = height as f32;
        self.width_px = width;
        self.height_px = height;
        match &mut self.target {
            CpuTarget::Window { surface, .. } => {
                if let (Some(w), Some(h)) =
                    (NonZeroU32::new(width), NonZeroU32::new(height))
                {
                    let _ = surface.resize(w, h);
                }
            }
            // The next frame repaints every pixel, so the old content
            // needn't be kept in place.
            CpuTarget::Offscreen { pixels, .. } => {
                pixels.clear();
                pixels.resize(width as usize * height as usize, 0);
            }
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn supports_f16(&self) -> bool {
        false
    }
}
