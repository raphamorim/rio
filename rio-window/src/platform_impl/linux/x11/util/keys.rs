use std::iter::Enumerate;
use std::slice::Iter;

use super::*;

pub struct Keymap {
    keys: [u8; 32],
}

pub struct KeymapIter<'a> {
    iter: Enumerate<Iter<'a, u8>>,
    index: usize,
    item: Option<u8>,
}

impl Keymap {
    pub fn iter(&self) -> KeymapIter<'_> {
        KeymapIter {
            iter: self.keys.iter().enumerate(),
            index: 0,
            item: None,
        }
    }
}

impl<'a> IntoIterator for &'a Keymap {
    type IntoIter = KeymapIter<'a>;
    type Item = ffi::KeyCode;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Iterator for KeymapIter<'_> {
    type Item = ffi::KeyCode;

    fn next(&mut self) -> Option<ffi::KeyCode> {
        if self.item.is_none() {
            for (index, &item) in self.iter.by_ref() {
                if item != 0 {
                    self.index = index;
                    self.item = Some(item);
                    break;
                }
            }
        }

        self.item.take().map(|item| {
            debug_assert!(item != 0);

            let bit = first_bit(item);

            if item != bit {
                // Remove the first bit; save the rest for further iterations
                self.item = Some(item ^ bit);
            }

            let shift = bit.trailing_zeros() + (self.index * 8) as u32;
            shift as ffi::KeyCode
        })
    }
}

impl XConnection {
    pub fn query_keymap(&self) -> Keymap {
        let mut keys = [0; 32];

        unsafe {
            (self.xlib.XQueryKeymap)(self.display, keys.as_mut_ptr() as *mut c_char);
        }

        Keymap { keys }
    }
}

fn first_bit(b: u8) -> u8 {
    1 << b.trailing_zeros()
}
