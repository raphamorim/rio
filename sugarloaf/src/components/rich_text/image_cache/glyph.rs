use super::cache::ImageCache;
use super::{AddImage, ImageData, ImageId, ImageLocation};
use core::borrow::Borrow;
use core::hash::{Hash, Hasher};
use rustc_hash::FxHashMap;
use swash::scale::{
    image::{Content, Image as GlyphImage},
    *,
};
use swash::zeno::Format;
use swash::FontRef;

const SOURCES: &[Source] = &[
    Source::ColorOutline(0),
    Source::ColorBitmap(StrikeWith::BestFit),
    // Source::Bitmap(StrikeWith::ExactSize),
    Source::Outline,
];

pub struct GlyphCache {
    scx: ScaleContext,
    fonts: FxHashMap<FontKey, FontEntry>,
    img: GlyphImage,
}

impl GlyphCache {
    pub fn new() -> Self {
        GlyphCache {
            scx: ScaleContext::new(),
            fonts: FxHashMap::default(),
            img: GlyphImage::new(),
        }
    }

    #[inline]
    pub fn session<'a>(
        &'a mut self,
        images: &'a mut ImageCache,
        font: FontRef<'a>,
        coords: &[i16],
        size: f32,
    ) -> GlyphCacheSession<'a> {
        // let quant_size = (size * 32.) as u16;
        let quant_size = size as u16;
        let entry = get_entry(&mut self.fonts, font.key.value(), coords);
        let scaler = self
            .scx
            .builder(font)
            // .hint(!IS_MACOS)
            .hint(true)
            .size(size)
            // .normalized_coords(coords)
            .build();
        GlyphCacheSession {
            entry,
            images,
            scaler,
            scaled_image: &mut self.img,
            quant_size,
        }
    }

    // pub fn prune(&mut self, images: &mut ImageCache) {
    //     self.fonts.retain(|_, entry| {
    //         for glyph in &entry.glyphs {
    //             images.deallocate(glyph.1.image);
    //         }
    //         false
    //     });
    // }

    #[allow(unused)]
    pub fn clear_evicted(&mut self, images: &mut ImageCache) {
        self.fonts.retain(|_, entry| {
            entry.glyphs.retain(|_, g| images.is_valid(g.image));
            !entry.glyphs.is_empty()
        });
    }
}

fn get_entry<'a>(
    fonts: &'a mut FxHashMap<FontKey, FontEntry>,
    id: u64,
    coords: &[i16],
) -> &'a mut FontEntry {
    let key = (id, Coords::Ref(coords));
    if let Some(entry) = fonts.get_mut(&key) {
        // Remove this unsafe when Rust learns that early returns should not
        // hold a borrow until the end of the function, or HashMap gets an
        // entry API that accepts borrowed keys (in which case, the double
        // lookup here can be removed altogether)
        return unsafe { core::mem::transmute::<&mut FontEntry, &mut FontEntry>(entry) };
    }
    let key = FontKey {
        key: (id, Coords::new(coords)),
    };
    fonts.entry(key).or_default()
}

pub struct GlyphCacheSession<'a> {
    entry: &'a mut FontEntry,
    images: &'a mut ImageCache,
    scaler: Scaler<'a>,
    scaled_image: &'a mut GlyphImage,
    quant_size: u16,
}

impl<'a> GlyphCacheSession<'a> {
    pub fn get_image(&mut self, image: ImageId) -> Option<ImageLocation> {
        self.images.get(&image)
    }

    #[inline]
    pub fn get(&mut self, id: u16) -> Option<GlyphEntry> {
        let key = GlyphKey {
            id,
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
            // .format(Format::Alpha)
            // .offset(Vector::new(subpx[0].to_f32(), subpx[1].to_f32()))
            // .embolden(embolden)
            // .transform(if cache_key.flags.contains(CacheKeyFlags::FAKE_ITALIC) {
            //     Some(Transform::skew(
            //         Angle::from_degrees(14.0),
            //         Angle::from_degrees(0.0),
            //     ))
            // } else {
            //     None
            // })
            .render_into(&mut self.scaler, id, self.scaled_image)
        {
            let p = self.scaled_image.placement;
            let w = p.width as u16;
            let h = p.height as u16;
            let req = AddImage {
                width: w,
                height: h,
                has_alpha: true,
                evictable: true,
                data: ImageData::Borrowed(&self.scaled_image.data),
            };
            let image = self.images.allocate(req)?;
            let entry = GlyphEntry {
                left: p.left,
                top: p.top,
                width: w,
                height: h,
                image,
                is_bitmap: self.scaled_image.content == Content::Color,
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
    glyphs: FxHashMap<GlyphKey, GlyphEntry>,
}

#[derive(Clone, Debug)]
#[repr(u8)]
enum Coords<'a> {
    None,
    Inline(u8, [i16; 8]),
    Heap(Vec<i16>),
    Ref(&'a [i16]),
}

impl<'a> Coords<'a> {
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
    // subpx: [SubpixelOffset; 2],
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
    // pub desc: DescenderRegion,
}
