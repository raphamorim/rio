#![allow(clippy::assertions_on_constants)]

use super::*;
use crate::icon::{Pixel, RgbaIcon, PIXEL_SIZE};

impl Pixel {
    pub fn to_packed_argb(&self) -> Cardinal {
        let mut cardinal = 0;
        assert!(CARDINAL_SIZE >= PIXEL_SIZE);
        let as_bytes = &mut cardinal as *mut _ as *mut u8;
        unsafe {
            *as_bytes.offset(0) = self.b;
            *as_bytes.offset(1) = self.g;
            *as_bytes.offset(2) = self.r;
            *as_bytes.offset(3) = self.a;
        }
        cardinal
    }
}

impl RgbaIcon {
    pub(crate) fn to_cardinals(&self) -> Vec<Cardinal> {
        assert_eq!(self.rgba.len() % PIXEL_SIZE, 0);
        let pixel_count = self.rgba.len() / PIXEL_SIZE;
        assert_eq!(pixel_count, (self.width * self.height) as usize);
        let mut data = Vec::with_capacity(pixel_count);
        data.push(self.width as Cardinal);
        data.push(self.height as Cardinal);
        let pixels = self.rgba.as_ptr() as *const Pixel;
        for pixel_index in 0..pixel_count {
            let pixel = unsafe { &*pixels.add(pixel_index) };
            data.push(pixel.to_packed_argb());
        }
        data
    }
}
