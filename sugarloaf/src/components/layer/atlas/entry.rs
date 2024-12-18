use crate::components::core::shapes::Size;
use crate::components::layer::atlas;

#[derive(Debug)]
pub enum Entry {
    Contiguous(atlas::Allocation),
    Fragmented {
        size: Size<u32>,
        fragments: Vec<Fragment>,
    },
}

impl Entry {
    pub fn size(&self) -> Size<u32> {
        match self {
            Entry::Contiguous(allocation) => allocation.size(),
            Entry::Fragmented { size, .. } => *size,
        }
    }
}

#[derive(Debug)]
pub struct Fragment {
    pub position: (u32, u32),
    pub allocation: atlas::Allocation,
}
