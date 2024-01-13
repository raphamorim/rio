//! Shared font data.

use std::io;
use std::path::Path;
use std::sync::{Arc, Weak};
use std::time::SystemTime;

#[derive(Debug)]
enum Inner {
    Mapped(memmap2::Mmap),
    Memory(Vec<u8>),
}

impl Inner {
    fn data(&self) -> &[u8] {
        match self {
            Self::Mapped(mmap) => &*mmap,
            Self::Memory(vec) => &vec,
        }
    }
}

/// Atomically reference counted, heap allocated or memory mapped buffer.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct SharedData {
    inner: Arc<Inner>,
}

/// Weak reference to shared data.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct WeakSharedData {
    inner: Weak<Inner>,
}

impl WeakSharedData {
    /// Upgrades the weak reference.
    pub fn upgrade(&self) -> Option<SharedData> {
        Some(SharedData {
            inner: self.inner.upgrade()?,
        })
    }
}

impl SharedData {
    /// Creates shared data from the specified bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            inner: Arc::new(Inner::Memory(data)),
        }
    }

    /// Creates shared data from the specified path.
    pub fn from_file(
        path: impl AsRef<Path>,
        mmap: bool,
        timestamp: Option<SystemTime>,
    ) -> Result<Self, io::Error> {
        let path = path.as_ref();
        if let Some(timestamp) = timestamp {
            let metadata = path.metadata()?;
            let current_timestamp = metadata.modified()?;
            if timestamp != current_timestamp {
                // Close enough
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
        }
        let inner = Arc::new(if mmap {
            let file = std::fs::File::open(path)?;
            let map = unsafe { memmap2::Mmap::map(&file)? };
            Inner::Mapped(map)
        } else {
            let data = std::fs::read(path)?;
            Inner::Memory(data)
        });
        Ok(Self { inner })
    }

    /// Creates a new weak reference to the data.
    pub fn downgrade(&self) -> WeakSharedData {
        WeakSharedData {
            inner: Arc::downgrade(&self.inner),
        }
    }

    /// Returns the underlying bytes of the data.
    pub fn as_bytes(&self) -> &[u8] {
        self.inner.data()
    }

    /// Returns the number of strong references to the data.
    pub fn strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
}

impl std::ops::Deref for SharedData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.inner.data()
    }
}

impl AsRef<[u8]> for SharedData {
    fn as_ref(&self) -> &[u8] {
        self.inner.data()
    }
}
