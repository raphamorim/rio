use image::{DynamicImage, GenericImageView};

use crate::components::core::shapes::Hasher;

use std::hash::{Hash, Hasher as _};
use std::path::PathBuf;
use std::sync::Arc;

/// A handle of some image data.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
    id: u64,
    data: Data,
}

impl Handle {
    /// Creates a new handler for the provided data.
    pub fn new(data: Data) -> Self {
        Self::from(data)
    }

    /// Returns the data into memory and returns it (for example if the data is just a path).
    pub fn load_image(&self) -> image::ImageResult<DynamicImage> {
        match &self.data {
            Data::Path(path) => image::open(path),
            Data::Image(img) => Ok(img.clone()),
        }
    }

    /// Returns the unique identifier of the [`Handle`].
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns a reference to the image [`Data`].
    pub fn data(&self) -> &Data {
        &self.data
    }
}

/// Creates a image [`Handle`] for the given data.
impl From<Data> for Handle {
    fn from(data: Data) -> Self {
        let mut hasher = Hasher::default();
        data.hash(&mut hasher);

        Self {
            id: hasher.finish(),
            data,
        }
    }
}

impl Hash for Handle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

/// A wrapper around raw image data.
///
/// It behaves like a `&[u8]`.
#[derive(Clone)]
pub struct Bytes(Arc<dyn AsRef<[u8]> + Send + Sync + 'static>);

impl Bytes {
    /// Creates new [`Bytes`] around `data`.
    pub fn new(data: impl AsRef<[u8]> + Send + Sync + 'static) -> Self {
        Self(Arc::new(data))
    }
}

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.as_ref().as_ref().fmt(f)
    }
}

impl std::hash::Hash for Bytes {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_ref().as_ref().hash(state);
    }
}

impl PartialEq for Bytes {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for Bytes {}

impl AsRef<[u8]> for Bytes {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref().as_ref()
    }
}

impl std::ops::Deref for Bytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        self.0.as_ref().as_ref()
    }
}

/// The data of a raster image.
#[derive(Clone, PartialEq)]
pub enum Data {
    /// File data
    Path(PathBuf),

    Image(DynamicImage),
}

impl Hash for Data {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Self::Path(path) => path.hash(state),
            Self::Image(img) => img.as_bytes().hash(state),
        }
    }
}

impl std::fmt::Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Data::Path(path) => write!(f, "Path({path:?})"),
            Data::Image(img) => {
                let (width, height) = img.dimensions();
                write!(f, "Image({width} * {height})")
            }
        }
    }
}
