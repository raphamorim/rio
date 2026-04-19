use super::cache::ImageCache;
use super::{AddImage, ImageData, ImageId, ImageLocation};
use crate::font::FontLibrary;
use crate::font_introspector::zeno::Format;
use crate::font_introspector::{
    scale::{
        image::{Content, Image as GlyphImage},
        *,
    },
    FontRef,
};
use core::borrow::Borrow;
use core::hash::{Hash, Hasher};
use rustc_hash::FxHashMap;
use tracing::debug;
use zeno::{Angle, Transform};

// const IS_MACOS: bool = cfg!(target_os = "macos");

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
    max_height: u16,
}

impl GlyphCache {
    pub fn new() -> Self {
        GlyphCache {
            scx: ScaleContext::new(),
            fonts: FxHashMap::default(),
            img: GlyphImage::new(),
            max_height: 0,
        }
    }

    #[inline]
    pub fn session<'a>(
        &'a mut self,
        images: &'a mut ImageCache,
        font: usize,
        font_library: &'a FontLibrary,
        coords: &[i16],
        size: f32,
    ) -> GlyphCacheSession<'a> {
        // let quant_size = (size * 32.) as u16;
        let quant_size = size as u16;
        let entry = get_entry(&mut self.fonts, font, coords);
        GlyphCacheSession {
            font,
            entry,
            images,
            font_library,
            max_height: &self.max_height,
            scaled_image: &mut self.img,
            quant_size,
            scale_context: &mut self.scx,
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
}

fn get_entry<'a>(
    fonts: &'a mut FxHashMap<FontKey, FontEntry>,
    id: usize,
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
    scaled_image: &'a mut GlyphImage,
    font: usize,
    font_library: &'a FontLibrary,
    scale_context: &'a mut ScaleContext,
    quant_size: u16,
    #[allow(unused)]
    max_height: &'a u16,
}

impl GlyphCacheSession<'_> {
    pub fn get_image(&mut self, image: ImageId) -> Option<ImageLocation> {
        self.images.get(&image)
    }

    pub fn get_atlas_index(&self, image: ImageId) -> Option<usize> {
        self.images.get_atlas_index(image)
    }

    #[inline]
    pub fn get(&mut self, id: u16) -> Option<GlyphEntry> {
        self.get_at_size(id, self.quant_size)
    }

    /// Look up a glyph rasterized at a specific pixel size instead of the
    /// session's nominal font size. Used to pre-rasterize PUA glyphs at
    /// `cells × font_size` so the compositor's constraint-fit pass only
    /// ever downscales — upscaling a bitmap atlas entry is blurry, so a
    /// 1-cell-wide Nerd Font glyph stretched into a 2-cell slot needs a
    /// fresh larger rasterization, not a stretched one.
    /// Results are cached under `(id, size)`, so repeated lookups at the
    /// same constraint size don't re-rasterize.
    #[inline]
    pub fn get_at_size(&mut self, id: u16, size: u16) -> Option<GlyphEntry> {
        let key = GlyphKey { id, size };
        if let Some(entry) = self.entry.glyphs.get(&key) {
            if self.images.is_valid(entry.image) {
                return Some(*entry);
            }
        }

        // Glyph Protocol override: when the session was built with the
        // reserved custom font id, route to the outline rasterizer
        // instead of the font's `glyf`/CFF/bitmap tables. The `id`
        // here is the `RegisteredGlyph::index` assigned at registration.
        if self.font == crate::font::glyph_registry::CUSTOM_GLYPH_FONT_ID {
            if let Some(entry) = self.rasterize_custom(id, size) {
                self.entry.glyphs.insert(key, entry);
                return Some(entry);
            }
            return None;
        }

        // Log cache miss for debugging
        debug!(
            "GlyphCache miss for glyph_id={} size={} font={}",
            id, size, self.font
        );

        self.scaled_image.data.clear();
        let font_library_data = self.font_library.inner.read();
        let enable_hint = font_library_data.hinting;
        let font_data = font_library_data.get(&self.font);
        let should_embolden = font_data.should_embolden;
        let should_italicize = font_data.should_italicize;

        if let Some((shared_data, offset, cache_key)) =
            font_library_data.get_data(&self.font)
        {
            let font_ref = FontRef {
                data: shared_data.as_ref(),
                offset,
                key: cache_key,
            };
            let mut scaler = self
                .scale_context
                .builder(font_ref)
                // With the advent of high-DPI displays (displays with >300 pixels per inch),
                // font hinting has become less relevant, as aliasing effects become
                // un-noticeable to the human eye.
                // As a result Apple's Quartz text renderer, which is targeted for Retina displays,
                // now ignores font hint information completely.
                // .hint(!IS_MACOS)
                .hint(enable_hint)
                .size(size.into())
                // .normalized_coords(coords)
                .build();

            // let embolden = if IS_MACOS { 0.25 } else { 0. };
            if Render::new(SOURCES)
                .format(Format::Alpha)
                // .offset(Vector::new(subpx[0].to_f32(), subpx[1].to_f32()))
                .embolden(if should_embolden { 0.5 } else { 0.0 })
                .transform(if should_italicize {
                    Some(Transform::skew(
                        Angle::from_degrees(14.0),
                        Angle::from_degrees(0.0),
                    ))
                } else {
                    None
                })
                .render_into(&mut scaler, id, self.scaled_image)
            {
                let p = self.scaled_image.placement;
                let w = p.width as u16;
                let h = p.height as u16;

                // Handle zero-sized glyphs (spaces, zero-width characters) efficiently
                if w == 0 || h == 0 {
                    let entry = GlyphEntry {
                        left: p.left,
                        top: p.top,
                        width: w,
                        height: h,
                        image: ImageId::empty(), // Use a special empty image ID
                        is_bitmap: false,
                    };
                    self.entry.glyphs.insert(key, entry);
                    return Some(entry);
                }

                // Use the appropriate content type and data format
                let (image_data, content_type) = match self.scaled_image.content {
                    Content::Mask => {
                        // Alpha format: use data directly for R8 texture
                        (
                            ImageData::Borrowed(&self.scaled_image.data),
                            super::ContentType::Mask,
                        )
                    }
                    Content::Color => {
                        // Already RGBA format
                        (
                            ImageData::Borrowed(&self.scaled_image.data),
                            super::ContentType::Color,
                        )
                    }
                    Content::SubpixelMask => {
                        // Subpixel format (should not happen with Format::Alpha)
                        (
                            ImageData::Borrowed(&self.scaled_image.data),
                            super::ContentType::Color,
                        )
                    }
                };

                let req = AddImage {
                    width: w,
                    height: h,
                    has_alpha: true,
                    data: image_data,
                    content_type,
                };
                let image = self.images.allocate(req)?;

                // let mut top = p.top;
                // let mut height = h;

                // If dimension is None it means that we are running
                // for the first time and in this case, we will obtain
                // what the next glyph entries should respect in terms of
                // top and height values
                //
                // e.g: Placement { left: 11, top: 42, width: 8, height: 50 }
                //
                // The calculation is made based on max_height
                // If the rect max height is 50 and the glyph height is 68
                // and 48 top, then (68 - 50 = 18) height as difference and
                // apply it to the top (bigger the top == up ^).
                // if self.max_height > &0 && &h > self.max_height {
                //     let difference = h - self.max_height;

                //     top -= difference as i32;
                //     height = *self.max_height;
                // }

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
        }

        None
    }

    /// Rasterize a Glyph Protocol registration at the given pixel size
    /// and allocate it into the atlas. Returns `None` when the registry
    /// has no entry for `id` (stale cache, registration cleared between
    /// lookup and render) or when decoding fails.
    ///
    /// `fmt=glyf` takes the zeno alpha-mask path → mask atlas. The
    /// colour formats take the ttf-parser COLR walker + tiny-skia
    /// RGBA path → color atlas. No monochrome fallback — if the
    /// colour renderer refuses a payload, the cell shows tofu so the
    /// application sees something's wrong and can fix the payload.
    fn rasterize_custom(&mut self, id: u16, size: u16) -> Option<GlyphEntry> {
        use crate::font::glyf_decode;
        use crate::font::glyph_registry::StoredPayload;
        use zeno::{Command, Mask, PathBuilder};

        let registry = {
            let lib = self.font_library.inner.read();
            lib.glyph_registry.clone()?
        };
        // `id` is the u8 slot index packed into a u16 glyph-id field.
        // Recover the codepoint so we can look up the outline.
        let slot = u8::try_from(id).ok()?;
        let cp = registry.cp_for_index(slot)?;
        let glyph = registry.get(cp)?;

        // Colour payloads route to the COLR rasteriser. Rasterisation
        // is cached upstream by `(glyph_id, size)` so a 1-2 ms CPU
        // pass per unique glyph-size pair is invisible to the user;
        // the result is uploaded to the colour atlas the same way a
        // system-font emoji would be.
        if let StoredPayload::ColrV0 { glyphs, colr, cpal }
        | StoredPayload::ColrV1 { glyphs, colr, cpal } = &glyph.payload
        {
            // Foreground for palette-index-0xFFFF / CurrentColor. The
            // compositor renders colour glyphs untinted, so rendering
            // "foreground" at white is the best we can do until the
            // actual fg colour is threaded through this call site.
            let fg = [0xFF, 0xFF, 0xFF, 0xFF];
            let raster = super::colr_raster::rasterize(
                glyphs, colr, cpal, glyph.upm, size, fg,
            )?;
            if raster.width == 0 || raster.height == 0 {
                return None;
            }
            let req = super::AddImage {
                width: raster.width,
                height: raster.height,
                has_alpha: true,
                data: super::ImageData::Borrowed(&raster.data),
                content_type: super::ContentType::Color,
            };
            let image = self.images.allocate(req)?;
            return Some(GlyphEntry {
                left: raster.left,
                top: raster.top,
                width: raster.width,
                height: raster.height,
                image,
                is_bitmap: true,
            });
        }

        let mono_bytes: &[u8] = match &glyph.payload {
            StoredPayload::Glyf { glyf } => glyf,
            StoredPayload::ColrV0 { .. } | StoredPayload::ColrV1 { .. } => {
                // Unreachable: the early return above handles both.
                return None;
            }
        };

        let outline = glyf_decode::decode(mono_bytes).ok()?;
        let cmds = outline.walk(glyph.upm, size as f32);
        if cmds.is_empty() {
            return None;
        }

        // Convert our neutral PathCmd sequence into zeno::Command.
        let mut z = Vec::<Command>::with_capacity(cmds.len());
        for cmd in &cmds {
            match *cmd {
                glyf_decode::PathCmd::MoveTo { x, y } => {
                    z.move_to([x, y]);
                }
                glyf_decode::PathCmd::LineTo { x, y } => {
                    z.line_to([x, y]);
                }
                glyf_decode::PathCmd::QuadTo { cx, cy, x, y } => {
                    z.quad_to([cx, cy], [x, y]);
                }
                glyf_decode::PathCmd::Close => {
                    z.close();
                }
            }
        }

        let (data, placement) = Mask::new(z.as_slice()).render();
        let w = placement.width as u16;
        let h = placement.height as u16;
        if w == 0 || h == 0 {
            return Some(GlyphEntry {
                left: placement.left,
                top: placement.top,
                width: w,
                height: h,
                image: super::ImageId::empty(),
                is_bitmap: false,
            });
        }

        let req = super::AddImage {
            width: w,
            height: h,
            has_alpha: true,
            data: super::ImageData::Borrowed(&data),
            content_type: super::ContentType::Mask,
        };
        let image = self.images.allocate(req)?;

        Some(GlyphEntry {
            left: placement.left,
            top: placement.top,
            width: w,
            height: h,
            image,
            is_bitmap: false,
        })
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct FontKey {
    key: (usize, Coords<'static>),
}

impl<'a> Borrow<(usize, Coords<'a>)> for FontKey {
    fn borrow(&self) -> &(usize, Coords<'a>) {
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

impl Coords<'_> {
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

impl Coords<'_> {
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
