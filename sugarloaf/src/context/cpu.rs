// Copyright (c) 2023-present, Raphael Amorim.
//
// CPU rendering backend context.
//
// Owns a softbuffer surface and exposes its `&mut [u32]` pixel buffer
// directly to the rasterizer. There is no intermediate pixmap or pixel
// format conversion — the rasterizer writes 0x00RRGGBB u32 values straight
// into the buffer that softbuffer presents.

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

pub type CpuSurface =
    softbuffer::Surface<Rc<SoftbufferHandle>, Rc<SoftbufferHandle>>;

pub struct CpuContext {
    pub size: SugarloafWindowSize,
    pub scale: f32,
    /// Buffer width in u32 elements. Row stride equals this.
    pub width_px: u32,
    pub height_px: u32,
    pub surface: CpuSurface,
    _handle: Rc<SoftbufferHandle>,
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

        if let (Some(w), Some(h)) =
            (NonZeroU32::new(width), NonZeroU32::new(height))
        {
            surface
                .resize(w, h)
                .expect("CPU backend: failed to size softbuffer surface");
        }

        Self {
            size,
            scale,
            width_px: width,
            height_px: height,
            surface,
            _handle: handle,
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
        if let (Some(w), Some(h)) =
            (NonZeroU32::new(width), NonZeroU32::new(height))
        {
            let _ = self.surface.resize(w, h);
        }
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn supports_f16(&self) -> bool {
        false
    }
}
