use super::cache::ImageCache;
use super::PixelFormat;
use super::{AddImage, Epoch, ImageData, ImageId, ImageLocation};
use core::borrow::Borrow;
use core::hash::{Hash, Hasher};
use std::collections::HashMap;
use swash::scale::{
    image::{Content, Image as GlyphImage},
    *,
};
use swash::zeno::{Format, Vector};
use swash::FontRef;

const IS_MACOS: bool = cfg!(target_os = "macos");

const SOURCES: &[Source] = &[
    Source::ColorBitmap(StrikeWith::BestFit),
    Source::ColorOutline(0),
    // Source::Bitmap(StrikeWith::ExactSize),
    Source::Outline,
];

pub struct GlyphCache {
    scx: ScaleContext,
    fonts: HashMap<FontKey, FontEntry>,
    img: GlyphImage,
}

impl GlyphCache {
    pub fn new() -> Self {
        GlyphCache {
            scx: ScaleContext::new(),
            fonts: HashMap::default(),
            img: GlyphImage::new(),
        }
    }

    pub fn session<'a>(
        &'a mut self,
        epoch: Epoch,
        images: &'a mut ImageCache,
        font: FontRef<'a>,
        coords: &[i16],
        size: f32,
    ) -> GlyphCacheSession<'a> {
        let quant_size = (size * 32.) as u16;
        let entry = get_entry(&mut self.fonts, font.key.value(), coords);
        entry.epoch = epoch;
        let scaler = self
            .scx
            .builder(font)
            .hint(!IS_MACOS)
            .size(size)
            .normalized_coords(coords)
            .build();
        GlyphCacheSession {
            entry,
            epoch,
            images,
            scaler,
            scaled_image: &mut self.img,
            quant_size,
        }
    }

    pub fn prune(&mut self, epoch: Epoch, images: &mut ImageCache) {
        if let Some(time) = epoch.0.checked_sub(8) {
            self.fonts.retain(|_, entry| {
                if entry.epoch.0 < time {
                    for glyph in &entry.glyphs {
                        images.deallocate(glyph.1.image);
                    }
                    false
                } else {
                    true
                }
            });
        }
    }

    #[allow(unused)]
    pub fn clear_evicted(&mut self, images: &mut ImageCache) {
        self.fonts.retain(|_, entry| {
            entry.glyphs.retain(|_, g| images.is_valid(g.image));
            !entry.glyphs.is_empty()
        });
    }
}

fn get_entry<'a>(
    fonts: &'a mut HashMap<FontKey, FontEntry>,
    id: u64,
    coords: &[i16],
) -> &'a mut FontEntry {
    let key = (id, Coords::Ref(coords));
    if let Some(entry) = fonts.get_mut(&key) {
        // Remove this unsafe when Rust learns that early returns should not
        // hold a borrow until the end of the function, or HashMap gets an
        // entry API that accepts borrowed keys (in which case, the double
        // lookup here can be removed altogether)
        return unsafe { core::mem::transmute(entry) };
    }
    let key = FontKey {
        key: (id, Coords::new(coords)),
    };
    fonts.entry(key).or_default()
}

pub struct GlyphCacheSession<'a> {
    entry: &'a mut FontEntry,
    epoch: Epoch,
    images: &'a mut ImageCache,
    scaler: Scaler<'a>,
    scaled_image: &'a mut GlyphImage,
    quant_size: u16,
}

impl<'a> GlyphCacheSession<'a> {
    pub fn get_image(&mut self, image: ImageId) -> Option<ImageLocation> {
        self.images.get(self.epoch, image)
    }

    pub fn get(&mut self, id: u16, x: f32, y: f32) -> Option<GlyphEntry> {
        let subpx = [SubpixelOffset::quantize(x), SubpixelOffset::quantize(y)];
        let key = GlyphKey {
            id,
            subpx,
            size: self.quant_size,
        };
        if let Some(entry) = self.entry.glyphs.get(&key) {
            if self.images.is_valid(entry.image) {
                return Some(*entry);
            }
        }
        self.scaled_image.data.clear();
        // let embolden = if IS_MACOS { 0.25 } else { 0. };
        if Render::new(SOURCES)
            .format(Format::CustomSubpixel([0.3, 0., -0.3]))
            .offset(Vector::new(subpx[0].to_f32(), subpx[1].to_f32()))
            // .embolden(embolden)
            .render_into(&mut self.scaler, id, self.scaled_image)
        {
            let p = self.scaled_image.placement;
            let w = p.width as u16;
            let h = p.height as u16;
            let req = AddImage {
                format: PixelFormat::Rgba8,
                width: w,
                height: h,
                has_alpha: true,
                evictable: true,
                data: ImageData::Borrowed(&self.scaled_image.data),
            };
            let image = self.images.allocate(self.epoch, req)?;
            let entry = GlyphEntry {
                left: p.left,
                top: p.top,
                width: w,
                height: h,
                image,
                is_bitmap: self.scaled_image.content == Content::Color,
                desc: DescenderRegion::new(self.scaled_image),
            };
            self.entry.glyphs.insert(key, entry);
            return Some(entry);
        }

        None
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct FontKey {
    key: (u64, Coords<'static>),
}

impl<'a> Borrow<(u64, Coords<'a>)> for FontKey {
    fn borrow(&self) -> &(u64, Coords<'a>) {
        &self.key
    }
}

#[derive(Default)]
struct FontEntry {
    epoch: Epoch,
    glyphs: HashMap<GlyphKey, GlyphEntry>,
}

#[derive(Clone, Debug)]
#[repr(u8)]
enum Coords<'a> {
    None,
    Inline(u8, [i16; 8]),
    Heap(Vec<i16>),
    Ref(&'a [i16]),
}

impl Coords<'static> {
    fn new(coords: &[i16]) -> Self {
        let len = coords.len();
        if len == 0 {
            Self::None
        } else if len <= 8 {
            let mut arr = [0i16; 8];
            arr[..len].copy_from_slice(coords);
            Self::Inline(len as u8, arr)
        } else {
            Self::Heap(coords.into())
        }
    }
}

impl<'a> Coords<'a> {
    fn as_ref(&self) -> &[i16] {
        match self {
            Self::None => &[],
            Self::Inline(len, arr) => &arr[..*len as usize],
            Self::Heap(vec) => vec,
            Self::Ref(slice) => slice,
        }
    }
}

impl PartialEq for Coords<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl Eq for Coords<'_> {}

impl Hash for Coords<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
struct GlyphKey {
    id: u16,
    subpx: [SubpixelOffset; 2],
    size: u16,
}

#[derive(Copy, Clone, Debug)]
pub struct GlyphEntry {
    pub left: i32,
    pub top: i32,
    pub width: u16,
    pub height: u16,
    pub image: ImageId,
    pub is_bitmap: bool,
    pub desc: DescenderRegion,
}

#[derive(Copy, Clone, Debug)]
pub struct DescenderRegion {
    start: u16,
    end: u16,
}

impl DescenderRegion {
    fn new(image: &GlyphImage) -> Self {
        let mut start = u16::MAX;
        let mut end = 0;
        let h = image.placement.height as i32;
        let w = image.placement.width as usize;
        let y1 = image.placement.top + 1;
        if y1 >= 0 && y1 < h && w < u16::MAX as usize {
            let y1 = y1 as usize;
            let y2 = h as usize;
            start = u16::MAX;
            if image.content == Content::Mask {
                for y in y1..y2 {
                    let mut has_ink = false;
                    let mut in_ink = false;
                    let offset = y * w;
                    if let Some(row) = image.data.get(offset..offset + w) {
                        for (i, alpha) in row.iter().enumerate() {
                            if *alpha != 0 {
                                if !has_ink {
                                    has_ink = true;
                                    start = start.min(i as u16);
                                }
                                in_ink = true;
                            } else if in_ink {
                                in_ink = false;
                                end = end.max(i as u16);
                            }
                        }
                    }
                    if in_ink {
                        end = w as u16;
                    }
                }
            } else {
                for y in y1..y2 {
                    let mut has_ink = false;
                    let mut in_ink = false;
                    let offset = y * w * 4;
                    if let Some(row) = image.data.get(offset..offset + w * 4) {
                        for (i, rgba) in row.chunks_exact(4).enumerate() {
                            if rgba[0] != 0
                                || rgba[1] != 0
                                || rgba[2] != 0
                                || rgba[3] != 0
                            {
                                if !has_ink {
                                    has_ink = true;
                                    start = start.min(i as u16);
                                }
                                in_ink = true;
                            } else if in_ink {
                                in_ink = false;
                                end = end.max(i as u16);
                            }
                        }
                    }
                    if in_ink {
                        end = w as u16;
                    }
                }
            }
        }
        Self { start, end }
    }

    pub fn range(&self) -> Option<(f32, f32)> {
        if self.start <= self.end {
            Some((self.start as f32, self.end as f32))
        } else {
            None
        }
    }
}

#[derive(Hash, Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum SubpixelOffset {
    Zero = 0,
    Quarter = 1,
    Half = 2,
    ThreeQuarters = 3,
}

impl SubpixelOffset {
    // Skia quantizes subpixel offsets into 1/4 increments.
    // Given the absolute position, return the quantized increment
    fn quantize(pos: f32) -> Self {
        // Following the conventions of Gecko and Skia, we want
        // to quantize the subpixel position, such that abs(pos) gives:
        // [0.0, 0.125) -> Zero
        // [0.125, 0.375) -> Quarter
        // [0.375, 0.625) -> Half
        // [0.625, 0.875) -> ThreeQuarters,
        // [0.875, 1.0) -> Zero
        // The unit tests below check for this.
        let apos = ((pos - pos.floor()) * 8.0) as i32;
        match apos {
            1..=2 => SubpixelOffset::Quarter,
            3..=4 => SubpixelOffset::Half,
            5..=6 => SubpixelOffset::ThreeQuarters,
            _ => SubpixelOffset::Zero,
        }
    }

    fn to_f32(self) -> f32 {
        match self {
            SubpixelOffset::Zero => 0.0,
            SubpixelOffset::Quarter => 0.25,
            SubpixelOffset::Half => 0.5,
            SubpixelOffset::ThreeQuarters => 0.75,
        }
    }
}
