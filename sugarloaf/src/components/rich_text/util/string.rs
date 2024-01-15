use core::borrow::Borrow;
use core::cmp::PartialEq;
use core::fmt;
use core::hash::{Hash, Hasher};
use std::sync::Arc;

const MAX_SMALL_SIZE: usize = 23;
const LEN_SLOT: usize = 23;
const INLINE_BIT: u8 = 0x80;
const LEN_MASK: u8 = !INLINE_BIT;

#[repr(align(8))]
pub struct SmallString {
    bytes: [u8; 24],
}

impl SmallString {
    pub fn new(s: &str) -> Self {
        let len = s.len();
        let mut bytes = [0; 24];
        if len <= MAX_SMALL_SIZE {
            (&mut bytes[..len]).copy_from_slice(s.as_bytes());
            bytes[LEN_SLOT] = (len as u8) | INLINE_BIT;
        } else {
            let arc: Arc<str> = s.into();
            unsafe {
                let ptr: (usize, usize) = core::mem::transmute(arc);
                *(bytes.as_mut_ptr() as *mut (usize, usize)) = ptr;
            }
        }
        Self { bytes }
    }

    pub fn len(&self) -> usize {
        self.as_str().len()
    }

    pub fn as_str(&self) -> &str {
        if self.is_inline() {
            let len = (self.bytes[LEN_SLOT] & LEN_MASK) as usize;
            unsafe { core::str::from_utf8_unchecked(&self.bytes[..len]) }
        } else {
            let arc = self.as_arc();
            let s = &*arc as *const str;
            core::mem::forget(arc);
            unsafe { &*s }
        }
    }

    fn is_inline(&self) -> bool {
        self.bytes[LEN_SLOT] & INLINE_BIT != 0
    }

    fn as_arc(&self) -> Arc<str> {
        debug_assert!(!self.is_inline());
        unsafe {
            let ptr = *(self.bytes.as_ptr() as *const [usize; 2]);
            core::mem::transmute(ptr)
        }
    }
}

impl Drop for SmallString {
    fn drop(&mut self) {
        if !self.is_inline() {
            drop(self.as_arc())
        }
    }
}

impl Clone for SmallString {
    fn clone(&self) -> Self {
        if !self.is_inline() {
            let arc = self.as_arc();
            core::mem::forget(Arc::clone(&arc));
            core::mem::forget(arc);
        }
        Self { bytes: self.bytes }
    }
}

impl Borrow<str> for SmallString {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq for SmallString {
    fn eq(&self, other: &Self) -> bool {
        self.as_str().eq(other.as_str())
    }
}

impl Eq for SmallString {}

impl PartialEq<&'_ str> for SmallString {
    fn eq(&self, other: &&'_ str) -> bool {
        self.as_str().eq(*other)
    }
}

impl PartialEq<str> for SmallString {
    fn eq(&self, other: &str) -> bool {
        self.as_str().eq(other)
    }
}

impl PartialEq<SmallString> for str {
    fn eq(&self, other: &SmallString) -> bool {
        self.eq(other.as_str())
    }
}

impl Hash for SmallString {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.as_str().hash(hasher)
    }
}

impl fmt::Debug for SmallString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

impl fmt::Display for SmallString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_str().fmt(f)
    }
}

pub struct LowercaseString {
    buf: [u8; 128],
    heap: String,
}

impl LowercaseString {
    pub fn new() -> Self {
        Self {
            buf: [0u8; 128],
            heap: Default::default(),
        }
    }

    pub fn get<'a>(&'a mut self, name: &str) -> Option<&'a str> {
        if name.len() <= self.buf.len() && name.is_ascii() {
            let mut end = 0;
            for c in name.as_bytes() {
                self.buf[end] = c.to_ascii_lowercase();
                end += 1;
            }
            std::str::from_utf8(&self.buf[..end]).ok()
        } else {
            self.heap = name.to_lowercase();
            Some(&self.heap)
        }
    }
}
