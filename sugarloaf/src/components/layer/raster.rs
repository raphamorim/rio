use crate::components::core::shapes::Size;
use crate::components::layer::atlas::{self, Atlas};
use crate::components::layer::image::{Data, Handle};
use tracing::debug;

use rustc_hash::{FxHashMap, FxHashSet};

/// Entry in cache corresponding to an image handle
#[derive(Debug)]
pub enum Memory {
    /// Image data on host
    Host(image_rs::ImageBuffer<image_rs::Rgba<u8>, Vec<u8>>),
    /// Storage entry
    Device(atlas::Entry),
    /// Image not found
    NotFound,
    /// Invalid image data
    Invalid,
}

impl Memory {
    /// Width and height of image
    pub fn dimensions(&self) -> Size<u32> {
        match self {
            Memory::Host(image) => {
                let (width, height) = image.dimensions();

                Size { width, height }
            }
            Memory::Device(entry) => entry.size(),
            Memory::NotFound => Size {
                width: 1,
                height: 1,
            },
            Memory::Invalid => Size {
                width: 1,
                height: 1,
            },
        }
    }
}

/// Caches image raster data
#[derive(Debug, Default)]
pub struct Cache {
    map: FxHashMap<u64, Memory>,
    hits: FxHashSet<u64>,
}

/// Tries to load an image by its [`Handle`].
pub fn load_image(handle: &Handle) -> image_rs::ImageResult<image_rs::DynamicImage> {
    match handle.data() {
        Data::Path(path) => {
            let image = image_rs::ImageReader::open(path)?.decode()?;
            Ok(image)
        }
        Data::Bytes(bytes) => {
            let image = image_rs::load_from_memory(bytes)?;
            Ok(image)
        }
        Data::Rgba {
            width,
            height,
            pixels,
        } => {
            if let Some(image) =
                image_rs::ImageBuffer::from_vec(*width, *height, pixels.to_vec())
            {
                Ok(image_rs::DynamicImage::ImageRgba8(image))
            } else {
                Err(image_rs::error::ImageError::Limits(
                    image_rs::error::LimitError::from_kind(
                        image_rs::error::LimitErrorKind::DimensionError,
                    ),
                ))
            }
        }
    }
}

impl Cache {
    /// Load image
    pub fn load(
        &mut self,
        handle: &crate::components::layer::image::Handle,
    ) -> &mut Memory {
        if self.contains(handle) {
            return self.get(handle).unwrap();
        }

        // Log cache miss for debugging
        debug!("RasterCache miss for image handle_id={}", handle.id());

        let memory = match load_image(handle) {
            Ok(image) => Memory::Host(image.to_rgba8()),
            Err(image_rs::error::ImageError::IoError(_)) => Memory::NotFound,
            Err(_) => Memory::Invalid,
        };

        self.insert(handle, memory);
        self.get(handle).unwrap()
    }

    /// Load image and upload raster data
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        handle: &crate::components::layer::image::Handle,
        atlas: &mut Atlas,
        context: &crate::context::Context,
    ) -> Option<&atlas::Entry> {
        let memory = self.load(handle);

        if let Memory::Host(image) = memory {
            let (width, height) = image.dimensions();

            let entry = atlas.upload(device, encoder, width, height, image, context)?;

            *memory = Memory::Device(entry);
        }

        if let Memory::Device(allocation) = memory {
            Some(allocation)
        } else {
            None
        }
    }

    /// Clear all cached images
    pub fn clear(&mut self) {
        self.map.clear();
        self.hits.clear();
    }

    /// Trim cache misses from cache
    pub fn trim(&mut self, atlas: &mut Atlas) {
        let hits = &self.hits;

        self.map.retain(|k, memory| {
            let retain = hits.contains(k);

            if !retain {
                if let Memory::Device(entry) = memory {
                    atlas.remove(entry);
                }
            }

            retain
        });

        self.hits.clear();
    }

    fn get(
        &mut self,
        handle: &crate::components::layer::image::Handle,
    ) -> Option<&mut Memory> {
        let _ = self.hits.insert(handle.id());

        self.map.get_mut(&handle.id())
    }

    fn insert(
        &mut self,
        handle: &crate::components::layer::image::Handle,
        memory: Memory,
    ) {
        let _ = self.map.insert(handle.id(), memory);
    }

    fn contains(&self, handle: &crate::components::layer::image::Handle) -> bool {
        self.map.contains_key(&handle.id())
    }
}
