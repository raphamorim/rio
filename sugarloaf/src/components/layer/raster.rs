use crate::components::core::shapes::Size;
use crate::components::layer::atlas::{self, Atlas};
use image as image_rs;

use std::collections::{HashMap, HashSet};

/// Entry in cache corresponding to an image handle
#[derive(Debug)]
pub enum Memory {
    /// Image data on host
    Host(image_rs::RgbaImage),
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
    map: HashMap<u64, Memory>,
    hits: HashSet<u64>,
}

impl Cache {
    /// Load image
    pub fn load_mut(
        &mut self,
        handle: &crate::components::layer::image::Handle,
    ) -> &mut Memory {
        if self.contains(handle) {
            return self.get_mut(handle).unwrap();
        }

        let memory = match handle.load_image() {
            Ok(img) => Memory::Host(img.to_rgba8()),
            Err(image_rs::error::ImageError::IoError(_)) => Memory::NotFound,
            Err(_) => Memory::Invalid,
        };

        self.insert(handle, memory);
        self.get_mut(handle).unwrap()
    }

    /// Load image and upload raster data
    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        handle: &crate::components::layer::image::Handle,
        atlas: &mut Atlas,
    ) -> Option<&atlas::Entry> {
        let memory = self.load_mut(handle);

        if let Memory::Host(img) = memory {
            let entry = atlas.upload(device, encoder, img)?;

            *memory = Memory::Device(entry);
        }

        if let Memory::Device(allocation) = memory {
            Some(allocation)
        } else {
            None
        }
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

    fn get_mut(
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
