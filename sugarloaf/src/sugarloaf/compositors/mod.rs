pub mod advanced;
pub mod elementary;

#[derive(Default)]
pub struct SugarCompositors {
    pub advanced: advanced::Advanced,
    pub elementary: elementary::Elementary,
}

#[derive(PartialEq, Debug, Clone, Default)]
pub enum SugarCompositorLevel {
    #[default]
    Elementary,
    Advanced,
}

impl SugarCompositorLevel {
    #[inline]
    pub fn is_advanced(&self) -> bool {
        self == &SugarCompositorLevel::Advanced
    }
}
