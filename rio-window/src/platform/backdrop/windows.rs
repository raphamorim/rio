use crate::backdrop::{BackdropProvider, PhysicalRect};

pub struct OsBackdropProvider;

impl OsBackdropProvider {
    pub fn new() -> Self {
        Self
    }
}

impl BackdropProvider for OsBackdropProvider {
    fn begin_frame(&mut self, _rect: PhysicalRect) -> Option<wgpu::TextureView> {
        // TODO: Implement DXGI Desktop Duplication capture
        None
    }
}
