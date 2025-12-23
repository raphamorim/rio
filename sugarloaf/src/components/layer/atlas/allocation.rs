use crate::components::layer::atlas::allocator;
use crate::components::layer::Size;

#[derive(Debug)]
pub enum Allocation {
    Partial {
        layer: usize,
        region: allocator::Region,
        #[allow(dead_code)]
        atlas_size: u32,
    },
    Full {
        layer: usize,
        #[allow(dead_code)]
        atlas_size: u32,
    },
}

impl Allocation {
    pub fn position(&self) -> (u32, u32) {
        match self {
            Allocation::Partial { region, .. } => region.position(),
            Allocation::Full { .. } => (0, 0),
        }
    }

    pub fn size(&self) -> Size<u32> {
        match self {
            Allocation::Partial { region, .. } => region.size(),
            Allocation::Full { atlas_size, .. } => Size {
                width: *atlas_size,
                height: *atlas_size,
            },
        }
    }

    pub fn layer(&self) -> usize {
        match self {
            Allocation::Partial { layer, .. } => *layer,
            Allocation::Full { layer, .. } => *layer,
        }
    }
}
