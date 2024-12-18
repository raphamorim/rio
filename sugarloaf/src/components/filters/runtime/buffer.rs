// This file was originally taken from https://github.com/SnowflakePowered/librashader
// SnowflakePowered/librashader is licensed under MPL-2.0
// https://github.com/SnowflakePowered/librashader/blob/master/LICENSE.md

use std::ops::{Deref, DerefMut};

pub struct WgpuStagedBuffer {
    buffer: wgpu::Buffer,
    shadow: Box<[u8]>,
}

impl WgpuStagedBuffer {
    pub fn new(
        device: &wgpu::Device,
        usage: wgpu::BufferUsages,
        size: wgpu::BufferAddress,
        label: wgpu::Label<'static>,
    ) -> WgpuStagedBuffer {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label,
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        WgpuStagedBuffer {
            buffer,
            shadow: vec![0u8; size as usize].into_boxed_slice(),
        }
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Write the contents of the backing buffer to the device buffer.
    pub fn flush(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.buffer, 0, &self.shadow);
    }
}

impl Deref for WgpuStagedBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.shadow.deref()
    }
}

impl DerefMut for WgpuStagedBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.shadow.deref_mut()
    }
}
