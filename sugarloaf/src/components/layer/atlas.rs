pub mod entry;

mod allocation;
mod allocator;
mod layer;

pub use allocation::Allocation;
pub use entry::Entry;
pub use layer::Layer;

use allocator::Allocator;

use crate::components::core::shapes::Size;

#[derive(Debug)]
pub struct Atlas {
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
    layers: Vec<Layer>,
    size: u32,
}

impl Atlas {
    pub fn new(
        device: &wgpu::Device,
        backend: wgpu::Backend,
        context: &crate::context::Context,
    ) -> Self {
        let max_size = context.max_texture_dimension_2d();
        let size = std::cmp::min(2048, max_size);

        tracing::info!("Creating layer atlas with size: {}x{} (reduced from 4096 for memory efficiency)", size, size);

        let layers = match backend {
            wgpu::Backend::Gl => vec![Layer::Empty, Layer::Empty],
            _ => vec![Layer::Empty],
        };

        let extent = wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: layers.len() as u32,
        };

        let texture_format = wgpu::TextureFormat::Rgba8Unorm;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image texture atlas"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        Atlas {
            texture,
            texture_view,
            layers,
            size,
        }
    }

    fn get_bytes_per_pixel(&self) -> u32 {
        match self.texture.format() {
            // 16-bit float formats (F16)
            wgpu::TextureFormat::R16Float => 2,
            wgpu::TextureFormat::Rg16Float => 4,
            wgpu::TextureFormat::Rgba16Float => 8,
            // 8-bit unorm formats
            wgpu::TextureFormat::R8Unorm => 1,
            wgpu::TextureFormat::Rg8Unorm => 2,
            wgpu::TextureFormat::Rgba8Unorm => 4,
            // Fallback for any unexpected format
            _ => {
                tracing::warn!(
                    "Unexpected texture format in atlas: {:?}, assuming 4 bytes per pixel",
                    self.texture.format()
                );
                4
            }
        }
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn view(&self) -> &wgpu::TextureView {
        &self.texture_view
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn clear(
        &mut self,
        device: &wgpu::Device,
        backend: wgpu::Backend,
        _context: &crate::context::Context,
    ) {
        self.layers = match backend {
            wgpu::Backend::Gl => vec![Layer::Empty, Layer::Empty],
            _ => vec![Layer::Empty],
        };

        let extent = wgpu::Extent3d {
            width: self.size,
            height: self.size,
            depth_or_array_layers: self.layers.len() as u32,
        };

        let texture_format = wgpu::TextureFormat::Rgba8Unorm;
        self.texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image texture atlas"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        self.texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });

        tracing::info!("Layer atlas cleared");
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        width: u32,
        height: u32,
        data: &[u8],
        _context: &crate::context::Context,
    ) -> Option<Entry> {
        let entry = {
            let current_size = self.layers.len();
            let entry = self.allocate(width, height)?;

            let new_layers = self.layers.len() - current_size;
            self.grow(new_layers, device, encoder);

            entry
        };

        tracing::info!("Allocated atlas entry: {:?}", entry);

        let bytes_per_pixel = self.get_bytes_per_pixel();
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let row_bytes = bytes_per_pixel * width;
        let padding = (align - row_bytes % align) % align;
        let padded_width = (row_bytes + padding) as usize;
        let padded_data_size = padded_width * height as usize;

        let mut padded_data = vec![0; padded_data_size];

        for row in 0..height as usize {
            let offset = row * padded_width;
            let src_row_bytes = (bytes_per_pixel * width) as usize;

            padded_data[offset..offset + src_row_bytes].copy_from_slice(
                &data[row * src_row_bytes..(row + 1) * src_row_bytes],
            )
        }

        match &entry {
            Entry::Contiguous(allocation) => {
                self.upload_allocation(
                    &padded_data,
                    (width, height),
                    padding,
                    0,
                    allocation,
                    (device, encoder),
                );
            }
            Entry::Fragmented { fragments, .. } => {
                for fragment in fragments {
                    let (x, y) = fragment.position;
                    let offset = (y * padded_width as u32 + bytes_per_pixel * x) as usize;

                    self.upload_allocation(
                        &padded_data,
                        (width, height),
                        padding,
                        offset,
                        &fragment.allocation,
                        (device, encoder),
                    );
                }
            }
        }

        tracing::info!("Current atlas: {:?}", self);

        Some(entry)
    }

    pub fn remove(&mut self, entry: &Entry) {
        tracing::info!("Removing atlas entry: {:?}", entry);

        match entry {
            Entry::Contiguous(allocation) => {
                self.deallocate(allocation);
            }
            Entry::Fragmented { fragments, .. } => {
                for fragment in fragments {
                    self.deallocate(&fragment.allocation);
                }
            }
        }
    }

    fn allocate(&mut self, width: u32, height: u32) -> Option<Entry> {
        let size = self.size;

        if width == size && height == size {
            let mut empty_layers = self
                .layers
                .iter_mut()
                .enumerate()
                .filter(|(_, layer)| layer.is_empty());

            if let Some((i, layer)) = empty_layers.next() {
                *layer = Layer::Full;

                return Some(Entry::Contiguous(Allocation::Full {
                    layer: i,
                    atlas_size: size,
                }));
            }

            self.layers.push(Layer::Full);

            return Some(Entry::Contiguous(Allocation::Full {
                layer: self.layers.len() - 1,
                atlas_size: size,
            }));
        }

        if width > size || height > size {
            let mut fragments = Vec::new();
            let mut y = 0;

            while y < height {
                let height = std::cmp::min(height - y, size);
                let mut x = 0;

                while x < width {
                    let width = std::cmp::min(width - x, size);

                    let allocation = self.allocate(width, height)?;

                    if let Entry::Contiguous(allocation) = allocation {
                        fragments.push(entry::Fragment {
                            position: (x, y),
                            allocation,
                        });
                    }

                    x += width;
                }

                y += height;
            }

            return Some(Entry::Fragmented {
                size: Size { width, height },
                fragments,
            });
        }

        for (i, layer) in self.layers.iter_mut().enumerate() {
            match layer {
                Layer::Empty => {
                    let mut allocator = Allocator::new(self.size);

                    if let Some(region) = allocator.allocate(width, height) {
                        *layer = Layer::Busy(allocator);

                        return Some(Entry::Contiguous(Allocation::Partial {
                            region,
                            layer: i,
                            atlas_size: self.size,
                        }));
                    }
                }
                Layer::Busy(allocator) => {
                    if let Some(region) = allocator.allocate(width, height) {
                        return Some(Entry::Contiguous(Allocation::Partial {
                            region,
                            layer: i,
                            atlas_size: self.size,
                        }));
                    }
                }
                _ => {}
            }
        }

        let mut allocator = Allocator::new(self.size);

        if let Some(region) = allocator.allocate(width, height) {
            self.layers.push(Layer::Busy(allocator));

            return Some(Entry::Contiguous(Allocation::Partial {
                region,
                layer: self.layers.len() - 1,
                atlas_size: self.size,
            }));
        }

        None
    }

    fn deallocate(&mut self, allocation: &Allocation) {
        tracing::info!("Deallocating atlas: {:?}", allocation);

        match allocation {
            Allocation::Full { layer, .. } => {
                self.layers[*layer] = Layer::Empty;
            }
            Allocation::Partial { layer, region, .. } => {
                let layer = &mut self.layers[*layer];

                if let Layer::Busy(allocator) = layer {
                    allocator.deallocate(region);

                    if allocator.is_empty() {
                        *layer = Layer::Empty;
                    }
                }
            }
        }
    }

    fn upload_allocation(
        &mut self,
        data: &[u8],
        image_dimensions: (u32, u32),
        padding: u32,
        offset: usize,
        allocation: &Allocation,
        context: (&wgpu::Device, &mut wgpu::CommandEncoder),
    ) {
        use wgpu::util::DeviceExt;

        let device = context.0;
        let encoder = context.1;

        let (x, y) = allocation.position();
        let Size { width, height } = allocation.size();
        let layer = allocation.layer();

        let extent = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };

        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("image upload buffer"),
            contents: data,
            usage: wgpu::BufferUsages::COPY_SRC,
        });

        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: offset as u64,
                    bytes_per_row: Some(
                        self.get_bytes_per_pixel() * image_dimensions.0 + padding,
                    ),
                    rows_per_image: Some(image_dimensions.1),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x,
                    y,
                    z: layer as u32,
                },
                aspect: wgpu::TextureAspect::default(),
            },
            extent,
        );
    }

    fn grow(
        &mut self,
        amount: usize,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        if amount == 0 {
            return;
        }

        let new_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("image texture atlas"),
            size: wgpu::Extent3d {
                width: self.size,
                height: self.size,
                depth_or_array_layers: self.layers.len() as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture.format(),
            usage: wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });

        let amount_to_copy = self.layers.len() - amount;

        for (i, layer) in self.layers.iter_mut().take(amount_to_copy).enumerate() {
            if layer.is_empty() {
                continue;
            }

            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::default(),
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &new_texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: i as u32,
                    },
                    aspect: wgpu::TextureAspect::default(),
                },
                wgpu::Extent3d {
                    width: self.size,
                    height: self.size,
                    depth_or_array_layers: 1,
                },
            );
        }

        self.texture = new_texture;
        self.texture_view = self.texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
    }
}
